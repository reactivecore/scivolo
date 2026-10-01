//! Expressions (nscplugin's `ExprBuffer.genExpr` and the trees without calls).

use super::Gen;
use super::apply::is_primitive;
use crate::symbols::{has, is_module, is_module_class};
use crate::{Result, unsupported};
use nir::{Op, Type, Val, ValId};
use syntax::ast::constant::Constant;
use tasty::SymbolId;
use tasty::tags::MUTABLE;
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    pub fn gen_expr(&mut self, t: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        match *self.l.tpd.tree(t) {
            Tree::Apply { fun, args } => self.gen_apply(t, fun, args),
            Tree::Block { stats, expr } => {
                let pos = self.pos_or(span, self.m.pos);
                let scope = self.enter_scope(pos);
                for s in self.l.tpd.items(stats).to_vec() {
                    self.gen_expr(s)?;
                }
                let v = self.gen_expr(expr)?;
                self.m.scope = scope;
                Ok(v)
            }
            Tree::Ident { .. } => self.gen_ident(t),
            Tree::If { cond, thenp, elsep } => {
                let retty = self.gen_type(self.l.tpd.tpe(t));
                self.gen_if(retty, cond, thenp, elsep)
            }
            Tree::Literal { ref value } => self.gen_literal(t, &value.clone()),
            Tree::ClassLiteral { tpe } => self.gen_class_literal(t, tpe),
            Tree::Select { qualifier, .. } => self.gen_select(t, qualifier),
            Tree::This { .. } => self.gen_this(t),
            Tree::Try { expr, cases, finalizer } => self.gen_try(t, expr, cases, finalizer),
            Tree::Typed { expr, .. } => match self.l.tpd.tree(expr) {
                Tree::Super { .. } => Ok(self.m.this.expect("`this` of a method")),
                _ => self.gen_expr(expr),
            },
            Tree::ValDef { rhs, .. } => self.gen_val_def(t, rhs),
            ref tree => unsupported(span, format!("{} in an expression", variant(tree))),
        }
    }

    fn gen_ident(&mut self, t: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let Some(sym) = self.l.tpd.sym(t) else { return unsupported(span, "an identifier without a symbol") };
        let value = if is_module(self.l.u, sym) {
            self.gen_module(sym)
        } else if let Some(&v) = self.m.env.get(&sym) {
            v
        } else {
            return unsupported(span, format!("a reference to {}", self.l.u.symbol(sym).name));
        };
        let ty = self.val_ty(value);
        if *self.pool.ty(ty) == Type::Nothing {
            self.raise(value);
            let unwind = self.unwind();
            self.emit_inst(nir::Inst::Unreachable(unwind));
        }
        Ok(value)
    }

    /// The instance of object `sym`, its value or its class.
    pub fn gen_module(&mut self, sym: SymbolId) -> ValId {
        let class = if is_module_class(self.l.u, sym) { Some(sym) } else { self.l.u.module_class(sym) };
        let name = self.module_name(class.unwrap_or(sym));
        self.emit(Op::Module(name))
    }

    fn gen_select(&mut self, t: TNodeId, qual: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let Some(sym) = self.l.tpd.sym(t) else { return unsupported(span, "a selection without a symbol") };
        if is_module(self.l.u, sym) {
            return Ok(self.gen_module(sym));
        }
        if self.l.u.symbol(sym).kind == tasty::Kind::Def {
            return self.gen_apply_method(sym, false, super::apply::Receiver::Tree(qual), &[]);
        }
        unsupported(span, "fields")
    }

    fn gen_this(&mut self, t: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let Some(sym) = self.l.tpd.sym(t) else { return unsupported(span, "`this` without a class") };
        let class = self.class.expect("a class");
        if sym != class && is_module_class(self.l.u, sym) {
            return Ok(self.gen_module(sym));
        }
        let method_owner = self.m.sym.and_then(|m| self.l.u.symbol(m).owner);
        match self.m.this {
            Some(this) if sym == class || method_owner == Some(class) => Ok(this),
            _ => unsupported(span, "`this` of an enclosing class"),
        }
    }

    fn gen_literal(&mut self, t: TNodeId, c: &Constant) -> Result<ValId> {
        let v = match *c {
            Constant::Unit => Val::Unit,
            Constant::Null => Val::Null,
            Constant::Boolean(b) => {
                if b {
                    Val::True
                } else {
                    Val::False
                }
            }
            Constant::Char(c) => Val::Char(c),
            Constant::Int(i) => Val::Int(i),
            Constant::Long(l) => Val::Long(l),
            Constant::Float(f) => Val::Float(f.to_bits()),
            Constant::Double(d) => Val::Double(d.to_bits()),
            Constant::String(ref units) => match String::from_utf16(units) {
                Ok(s) => Val::String(self.str(&s)),
                Err(_) => return unsupported(self.l.tpd.span(t), "strings that are not valid Unicode"),
            },
        };
        Ok(self.val(v))
    }

    /// `classOf[tpe]`: the class of an object for its singleton type.
    fn gen_class_literal(&mut self, t: TNodeId, tpe: tasty::TypeId) -> Result<ValId> {
        let class = match *self.l.u.types().get(tpe) {
            tasty::Type::Sym(_, s) if is_module(self.l.u, s) => self.l.u.module_class(s),
            _ => self.l.u.class_symbol(tpe),
        };
        match class {
            Some(c) if !is_primitive(&self.l.u.binary_name(c)) => {
                let name = self.type_name(c);
                Ok(self.val(Val::ClassOf(name)))
            }
            _ => unsupported(self.l.tpd.span(t), "classOf of a type that is not a class"),
        }
    }

    fn gen_val_def(&mut self, t: TNodeId, rhs: Option<TNodeId>) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let sym = self.l.tpd.sym(t).expect("a value definition's symbol");
        if has(self.l.u, sym, MUTABLE) {
            return unsupported(span, "local variables");
        }
        let saved = self.at_span(span);
        let name = self.local_name(sym);
        let value = match rhs {
            Some(rhs) => self.gen_expr(rhs)?,
            None => self.val(Val::Unit),
        };
        let value = match *self.pool.val(value) {
            Val::Local(id, _) => {
                if !self.m.local_names.iter().any(|(l, _)| *l == id) {
                    self.name_local(id, &name);
                }
                value
            }
            Val::Unit => value,
            _ => self.emit_named(Op::Copy(value), &name),
        };
        self.m.env.insert(sym, value);
        self.m.pos = saved;
        Ok(self.val(Val::Unit))
    }
}

/// The name of `tree`'s class.
fn variant(tree: &Tree) -> String {
    let s = format!("{tree:?}");
    s[..s.find([' ', '{', '(']).unwrap_or(s.len())].to_owned()
}
