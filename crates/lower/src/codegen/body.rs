//! Methods: their names, signatures and bodies (nscplugin's `genMethod` and
//! `genMethodBody`).

use super::Gen;
use super::dead::remove_dead_blocks;
use super::method::Method;
use crate::phases::walk::children;
use crate::symbols::{has, is_extern, is_java_static};
use crate::{Result, unsupported};
use nir::{Attr, Body, Defn, DefnKind, LexicalScope, Op, Param, Type, Val, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use tasty::tags::{FIELDaccessor, INLINE};
use typer::tpd::{Clauses, TNodeId, Tree};

impl Gen<'_, '_> {
    pub fn gen_method(&mut self, dd: TNodeId) -> Result<Defn> {
        let sym = self.l.tpd.sym(dd).expect("a method definition's symbol");
        let Tree::DefDef { paramss, rhs, .. } = *self.l.tpd.tree(dd) else { unreachable!("a method") };
        self.check_method(sym, self.l.tpd.span(dd))?;
        let pos = self.pos_or(self.l.tpd.span(dd), self.no_pos);
        let name = self.method_name(sym);
        let ty = self.method_sig(sym, false);
        let inline = has(self.l.u, sym, FIELDaccessor)
            || self.l.bridges.contains(&sym)
            || self.l.local_lazy_accessors.contains(&sym);
        let attrs = if inline { vec![Attr::AlwaysInline] } else { Vec::new() };
        let Some(rhs) = rhs else {
            return Ok(Defn { name, attrs, pos, kind: DefnKind::Declare { ty } });
        };
        let first_scope = if matches!(self.l.tpd.tree(rhs), Tree::Block { .. }) { -1 } else { 0 };
        self.m = Method::new(Some(sym), first_scope, pos);
        let rhs_pos = self.pos_or(self.l.tpd.span(rhs), pos);
        self.m.scopes.push(LexicalScope { id: 0, parent: 0, pos: rhs_pos });
        self.m.pos = rhs_pos;
        let body = self.gen_method_body(sym, paramss, rhs)?;
        Ok(Defn { name, attrs, pos, kind: DefnKind::Define { ty, body } })
    }

    /// Fails for a method the lowering cannot define yet.
    fn check_method(&mut self, sym: SymbolId, span: Span) -> Result<()> {
        if has(self.l.u, sym, INLINE) {
            return unsupported(span, "inline methods");
        }
        if is_extern(self.l.u, sym) {
            return unsupported(span, "native interop");
        }
        Ok(())
    }

    fn gen_method_body(&mut self, sym: SymbolId, paramss: Clauses, rhs: TNodeId) -> Result<Body> {
        let clauses = self.l.tpd.clause_items(paramss).to_vec();
        let param_syms: Vec<SymbolId> = clauses
            .iter()
            .flat_map(|&c| self.l.tpd.items(c))
            .filter(|&&p| matches!(self.l.tpd.tree(p), Tree::ValDef { .. }))
            .filter_map(|&p| self.l.tpd.sym(p))
            .collect();
        let mut params = Vec::new();
        for p in param_syms {
            let info = self.l.u.info(p);
            let ty = self.value_type(info);
            let name = self.local_name(p);
            let id = self.fresh_named(&name);
            let v = self.val(Val::Local(id, ty));
            self.m.env.insert(p, v);
            params.push(Param { id, ty });
        }
        if !is_java_static(self.l.u, sym) {
            let class = self.class.expect("a method's class");
            let ty = self.class_ref(class);
            let id = self.fresh_named("this");
            self.m.this = Some(self.val(Val::Local(id, ty)));
            params.insert(0, Param { id, ty });
        }
        let entry = self.fresh();
        self.label(entry, params);
        for var in self.mutable_vars(rhs) {
            let info = self.l.u.info(var);
            let ty = self.gen_type(info);
            let name = self.local_name(var);
            let slot = self.emit_named(Op::Var(ty), &name);
            self.m.env.insert(var, slot);
            self.m.mutable.insert(var);
        }
        let value = self.gen_expr(rhs)?;
        self.gen_return(sym, value, self.l.tpd.span(rhs))?;
        let pos = self.m.pos;
        let m = std::mem::replace(&mut self.m, Method::new(None, 0, pos));
        let buf = remove_dead_blocks(m.buf);
        Ok(Body { insts: buf.insts, positions: buf.positions, local_names: m.local_names, scopes: m.scopes })
    }

    /// The local variables assigned in `rhs`, in the order of their first assignment (nscplugin's
    /// `CollectMethodInfo`).
    fn mutable_vars(&self, rhs: TNodeId) -> Vec<SymbolId> {
        let mut out = Vec::new();
        let mut todo = vec![rhs];
        while let Some(t) = todo.pop() {
            if let Tree::Assign { lhs, .. } = *self.l.tpd.tree(t)
                && let (Tree::Ident { .. }, Some(var)) = (self.l.tpd.tree(lhs), self.l.tpd.sym(lhs))
                && !self.is_field(var)
                && !out.contains(&var)
            {
                out.push(var);
            }
            todo.extend(children(self.l.tpd, t).into_iter().rev());
        }
        out
    }

    /// `ret value`, `ret unit` for a `Unit` value.
    pub fn gen_return(&mut self, sym: SymbolId, value: ValId, span: Span) -> Result<()> {
        let sig = self.method_sig(sym, false);
        let Type::Function(_, result) = *self.pool.ty(sig) else { unreachable!("a method type") };
        let value = match *self.pool.val(value) {
            Val::Zero(_) => self.val(Val::Zero(result)),
            _ => value,
        };
        let ty = self.val_ty(value);
        if self.m.buf.labeled() && *self.pool.ty(result) != Type::Unit {
            self.check_conforms(value, result, span)?;
        }
        let value = if *self.pool.ty(ty) == Type::Unit { self.val(Val::Unit) } else { value };
        self.ret(value);
        Ok(())
    }
}
