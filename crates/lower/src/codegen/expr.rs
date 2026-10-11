//! Expressions (nscplugin's `ExprBuffer.genExpr` and the trees without calls).

use super::Gen;
use crate::phases::select_static::is_static_enum_value;
use crate::symbols::{has, is_static};
use crate::{Result, unsupported};
use nir::{Op, Type, Val, ValId};
use tasty::tags::{FIELDaccessor, LAZY};
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
            Tree::SeqLiteral { elems, elemtpt } => self.gen_array_literal(t, elems, elemtpt),
            Tree::TypeApply { fun, .. } => self.gen_cast(t, fun),
            Tree::Closure { env, meth, tpt } => self.gen_closure(t, env, meth, tpt),
            Tree::This { .. } => self.gen_this(t),
            Tree::Try { expr, cases, finalizer } => self.gen_try(t, expr, cases, finalizer),
            Tree::WhileDo { cond, body } => self.gen_while(t, cond, body),
            Tree::Labeled { expr } => self.gen_labeled(t, expr),
            Tree::Match { selector, cases } => self.gen_switch(t, selector, cases),
            Tree::Return { expr, from } => {
                if let Some(result) = self.gen_label_return(t, expr, from) {
                    return result;
                }
                let Some(method) = self.l.tpd.sym(from).filter(|&m| Some(m) == self.m.sym) else {
                    return unsupported(span, "returns from enclosing methods or labels");
                };
                let saved = self.at_span(span);
                let value = self.gen_expr(expr)?;
                self.gen_return(method, value, span)?;
                self.m.pos = saved;
                Ok(self.val(Val::Unit))
            }
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
        } else if self.l.u.is_module(sym) {
            self.gen_module(sym)
        } else if let Some(&slot) = self.m.env.get(&sym).filter(|_| self.m.mutable.contains(&sym)) {
            self.emit(Op::Varload(slot))
        } else if let Some(&v) = self.m.env.get(&sym) {
            v
        } else if self.is_field(sym) {
            self.gen_field_load(t, sym, None)?
        } else if is_static_enum_value(self.l, sym) {
            self.gen_apply_static(sym, None, &[], span)?
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

    fn gen_select(&mut self, t: TNodeId, qual: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let Some(sym) = self.l.tpd.sym(t) else { return unsupported(span, "a selection without a symbol") };
        let owner = self.l.u.symbol(sym).owner.expect("a member's owner");
        let accessor = !is_static(self.l.u, sym);
        if self.l.u.is_module(sym) && !accessor && self.l.u.symbol(owner).kind != tasty::Kind::Package {
            return self.gen_nested_module(t, sym, qual);
        }
        let saved = self.at_span(span);
        let v = if self.l.u.is_module(sym) && !accessor {
            self.gen_module(sym)
        } else if self.is_field(sym) {
            self.gen_field_load(t, sym, Some(qual))?
        } else if self.l.u.symbol(sym).kind == tasty::Kind::Def
            || has(self.l.u, sym, FIELDaccessor)
            || self.l.u.symbol(sym).tree.is_some()
        {
            self.gen_apply_method(sym, false, super::apply::Receiver::Tree(qual), &[], span)?
        } else {
            return unsupported(span, "a selection of a value that is not a field");
        };
        self.m.pos = saved;
        Ok(v)
    }

    fn gen_val_def(&mut self, t: TNodeId, rhs: Option<TNodeId>) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let sym = self.l.tpd.sym(t).expect("a value definition's symbol");
        let mutable = self.m.mutable.contains(&sym);
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
                if !mutable && !self.m.local_names.iter().any(|(l, _)| *l == id) {
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
        if mutable {
            let slot = self.m.env[&sym];
            self.emit(Op::Varstore { slot, value });
        } else {
            self.m.env.insert(sym, value);
        }
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
