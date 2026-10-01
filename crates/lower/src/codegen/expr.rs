//! Expressions (nscplugin's `ExprBuffer.genExpr` and the trees without calls).

use super::Gen;
use crate::symbols::{has, is_module, is_module_class, is_package_object};
use crate::{Result, unsupported};
use nir::{Op, Type, Val, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use tasty::tags::{FIELDaccessor, LAZY, MUTABLE};
use typer::tpd::print::class_name;
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
            Tree::Assign { lhs, rhs } => self.gen_assign(t, lhs, rhs),
            ref tree => unsupported(span, format!("{} in an expression", class_name(tree))),
        }
    }

    fn gen_ident(&mut self, t: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let Some(sym) = self.l.tpd.sym(t) else { return unsupported(span, "an identifier without a symbol") };
        let saved = self.at_span(span);
        let value = if sym == self.l.defs.boxed_unit_unit {
            self.val(Val::Unit)
        } else if is_module(self.l.u, sym) {
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
        self.m.pos = saved;
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
        let saved = self.at_span(span);
        let v = if is_module(self.l.u, sym) {
            self.gen_module(sym)
        } else if self.is_field(sym) {
            self.gen_field_load(t, sym, qual)?
        } else if self.l.u.symbol(sym).kind == tasty::Kind::Def || has(self.l.u, sym, FIELDaccessor) {
            self.gen_apply_method(sym, false, super::apply::Receiver::Tree(qual), &[], span)?
        } else {
            return unsupported(span, "a selection of a value that is not a field");
        };
        self.m.pos = saved;
        Ok(v)
    }

    fn gen_this(&mut self, t: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let Some(sym) = self.l.tpd.sym(t) else { return unsupported(span, "`this` without a class") };
        let saved = self.at_span(span);
        let v = self.this_of(sym, span)?;
        self.m.pos = saved;
        Ok(v)
    }

    /// `C.this` for class `c`: an object's instance, or the method's `this`.
    pub fn this_of(&mut self, c: SymbolId, span: Span) -> Result<ValId> {
        let class = self.class.expect("a class");
        let package_object = is_package_object(self.l.u, c);
        if (c != class && is_module_class(self.l.u, c)) || package_object {
            return Ok(self.gen_module(c));
        }
        let method_owner = self.m.sym.and_then(|m| self.l.u.symbol(m).owner);
        match self.m.this {
            Some(this) if c == class || method_owner == Some(class) => Ok(this),
            _ => unsupported(span, "`this` of an enclosing class"),
        }
    }

    fn gen_val_def(&mut self, t: TNodeId, rhs: Option<TNodeId>) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let sym = self.l.tpd.sym(t).expect("a value definition's symbol");
        if has(self.l.u, sym, MUTABLE) {
            return unsupported(span, "local variables");
        }
        if has(self.l.u, sym, LAZY) {
            return unsupported(span, "local lazy values");
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
                if rhs.is_some_and(|r| self.ends_in_block(r)) {
                    self.set_let_scope(id);
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

    /// Whether `t` is a block, possibly typed or inside a `try`.
    fn ends_in_block(&self, t: TNodeId) -> bool {
        match *self.l.tpd.tree(t) {
            Tree::Block { .. } => true,
            Tree::Typed { expr, .. } => matches!(self.l.tpd.tree(expr), Tree::Block { .. }),
            Tree::Try { expr, .. } => match *self.l.tpd.tree(expr) {
                Tree::Block { .. } => true,
                Tree::Typed { expr, .. } => matches!(self.l.tpd.tree(expr), Tree::Block { .. }),
                _ => false,
            },
            _ => false,
        }
    }
}
