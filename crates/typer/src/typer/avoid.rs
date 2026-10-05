//! Types without references to local symbols (dotty's `ensureNoLocalRefs`, `TypeOps.avoid`).

use super::{Pt, Typer};
use crate::tpd::{TNodeId, Tree as T};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether type `t` refers to one of `syms`.
    fn refers_to(&mut self, t: TypeId, syms: &[SymbolId]) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            if let Type::Sym(_, s) = u.types().get(part)
                && syms.contains(s)
            {
                found = true;
            }
            None
        });
        found
    }

    /// Block result `expr`, ascribed a type without references to `locals` if its own has some
    /// (dotty's `ensureNoLocalRefs`): its references to local terms widened, or `pt` if that
    /// does not conform to `pt`; `None` if a reference remains.
    pub fn no_local_refs(&mut self, expr: TNodeId, locals: &[SymbolId], pt: Pt) -> Option<TNodeId> {
        let tpe = self.tpd.tpe(expr);
        let tpe = self.zonk(tpe);
        if !self.refers_to(tpe, locals) {
            return Some(expr);
        }
        let mut avoided = self.u.map_type(tpe, &mut |u, part| {
            let pre = match u.types().get(part) {
                Type::Sym(Some(pre), _) | Type::TypeRef(pre, _) => *pre,
                _ => return None,
            };
            let mut local = false;
            u.map_type(pre, &mut |u, q| {
                local |= matches!(*u.types().get(q), Type::Sym(_, l) if locals.contains(&l));
                None
            });
            let dealiased = u.dealias(part);
            (local && dealiased != part).then_some(dealiased)
        });
        let mut checked = avoided != tpe;
        for _ in 0..16 {
            let Type::Sym(_, s) = *self.u.types().get(avoided) else { break };
            if !locals.contains(&s) || self.is_type_symbol(s) {
                break;
            }
            let info = self.u.info(s);
            let info = match *self.u.types().get(info) {
                Type::ByName(r) => r,
                _ => info,
            };
            avoided = self.u.dealias(info);
        }
        if let Pt::Type(p) = pt
            && self.is_fully_defined(p)
            && !self.u.is_subtype(avoided, p)
        {
            (avoided, checked) = (p, true);
        }
        if self.refers_to(avoided, locals) || checked && !self.u.is_subtype(tpe, avoided) {
            return None;
        }
        Some(self.ascribed(expr, avoided))
    }

    /// `expr: tpe`, the ascription moved into the result of a block (dotty's `ascribeType`).
    fn ascribed(&mut self, expr: TNodeId, tpe: TypeId) -> TNodeId {
        let span = self.tpd.span(expr);
        if let T::Block { stats, expr: last } = *self.tpd.tree(expr)
            && !matches!(self.tpd.tree(last), T::Closure { .. })
        {
            let last = self.ascribed(last, tpe);
            return self.tpd.node(T::Block { stats, expr: last }, span, Some(tpe), None);
        }
        let tpt = self.type_tree(tpe, span.end_pos(), true);
        self.tpd.node(T::Typed { expr, tpt }, span.to_synthetic(), Some(tpe), None)
    }
}
