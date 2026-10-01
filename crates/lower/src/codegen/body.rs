//! Methods: their names, signatures and bodies (nscplugin's `genMethod` and
//! `genMethodBody`).

use super::Gen;
use super::dead::remove_dead_blocks;
use super::method::Method;
use crate::symbols::{has, has_by_name_params, is_extern, is_java_static};
use crate::{Result, unsupported};
use nir::{Attr, Body, Defn, DefnKind, LexicalScope, Param, Type, Val, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use tasty::tags::{FIELDaccessor, INLINE};
use typer::tpd::{Clauses, TNodeId, Tree};

impl Gen<'_, '_> {
    pub fn gen_method(&mut self, dd: TNodeId) -> Result<Defn> {
        let sym = self.l.tpd.sym(dd).expect("a method definition's symbol");
        let Tree::DefDef { paramss, rhs, .. } = *self.l.tpd.tree(dd) else { unreachable!("a method") };
        self.check_method(sym, paramss, self.l.tpd.span(dd))?;
        let pos = self.pos_or(self.l.tpd.span(dd), self.no_pos);
        let name = self.method_name(sym);
        let ty = self.method_sig(sym, false);
        let attrs = if has(self.l.u, sym, FIELDaccessor) { vec![Attr::AlwaysInline] } else { Vec::new() };
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
    fn check_method(&mut self, sym: SymbolId, paramss: Clauses, span: Span) -> Result<()> {
        if has(self.l.u, sym, INLINE) {
            return unsupported(span, "inline methods");
        }
        if is_extern(self.l.u, sym) {
            return unsupported(span, "native interop");
        }
        if has_by_name_params(self.l.u, sym) {
            return unsupported(span, "by-name parameters");
        }
        let value_clauses = self.l.tpd.clause_items(paramss).iter().filter(|&&c| {
            let items = self.l.tpd.items(c);
            items.is_empty() || !items.iter().all(|&p| matches!(self.l.tpd.tree(p), Tree::TypeDef { .. }))
        });
        if value_clauses.count() > 1 {
            return unsupported(span, "several parameter lists");
        }
        Ok(())
    }

    fn gen_method_body(&mut self, sym: SymbolId, paramss: Clauses, rhs: TNodeId) -> Result<Body> {
        let first = self.l.tpd.clause_items(paramss).first().copied();
        let param_syms: Vec<SymbolId> = first
            .map(|c| self.l.tpd.items(c).iter().filter_map(|&p| self.l.tpd.sym(p)).collect())
            .unwrap_or_default();
        let mut params = Vec::new();
        for p in param_syms {
            let info = self.l.u.info(p);
            let ty = self.gen_type(info);
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
        let value = self.gen_expr(rhs)?;
        self.gen_return(sym, value, self.l.tpd.span(rhs))?;
        let pos = self.m.pos;
        let m = std::mem::replace(&mut self.m, Method::new(None, 0, pos));
        let buf = remove_dead_blocks(m.buf);
        Ok(Body { insts: buf.insts, positions: buf.positions, local_names: m.local_names, scopes: m.scopes })
    }

    /// `ret value`, `ret unit` for a `Unit` value.
    fn gen_return(&mut self, sym: SymbolId, value: ValId, span: Span) -> Result<()> {
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
