//! Types without references to local symbols (dotty's `ensureNoLocalRefs`, `TypeOps.avoid`).

use super::avoid_map::Avoided;
use super::{Pt, Typer};
use crate::tpd::{TNodeId, Tree as T};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether type `t` refers to one of `syms`.
    pub fn refers_to(&mut self, t: TypeId, syms: &[SymbolId]) -> bool {
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

    /// `{ defs; tree }`, or `tree` if there are no definitions (dotty's `wrapDefs`), its type
    /// avoiding the definitions (dotty's `avoidingType`); outside the subset if it still refers
    /// to them.
    pub fn wrap_defs(&mut self, defs: Vec<TNodeId>, tree: TNodeId) -> TNodeId {
        if defs.is_empty() {
            return tree;
        }
        let span = defs.iter().fold(self.tpd.span(tree), |s, &d| s.union(self.tpd.span(d)));
        let span = syntax::ast::Span::synthetic(span.start(), span.end());
        let syms: Vec<SymbolId> = defs.iter().filter_map(|&d| self.tpd.sym(d)).collect();
        let avoided = Avoided { locals: &syms, opaque: &[] };
        let tpe = self.tpd.get(tree).tpe.map(|t| match self.refers_to(t, &syms) {
            true => self.avoid_at(t, 1, avoided).map_or(t, |a| a.hi()),
            false => t,
        });
        if tpe.is_some_and(|t| self.refers_to(t, &syms)) {
            self.unsupported_at(span, "type of a lifted call referring to a lifted value");
        }
        let stats = self.tpd.list(&defs);
        self.tpd.node(T::Block { stats, expr: tree }, span, tpe, None)
    }

    /// Block result `expr`, ascribed a type without references to `locals` if its own has some
    /// (dotty's `ensureNoLocalRefs`): the upper approximation of `TypeOps.avoid`, or `pt` if that
    /// does not conform to `pt`; `None` if a reference remains, the approximation is out of the
    /// subset, or a type of `opaque`, whose bounds are unknown, is reached.
    pub fn no_local_refs(
        &mut self,
        expr: TNodeId,
        locals: &[SymbolId],
        opaque: &[SymbolId],
        pt: Pt,
    ) -> Option<TNodeId> {
        let tpe = self.tpd.tpe(expr);
        let tpe = self.zonk(tpe);
        if !self.escapes(tpe, locals) && !self.var_bounds_refer_to(tpe, locals) {
            return Some(expr);
        }
        let mut avoided = self.avoid_at(tpe, 1, Avoided { locals, opaque })?.hi();
        if let Pt::Type(p) = pt
            && let p = self.zonk(p)
            && self.is_fully_defined(p)
            && !self.u.is_subtype(avoided, p)
        {
            avoided = p;
        }
        if self.refers_to(avoided, locals)
            || self.var_bounds_refer_to(avoided, locals)
            || !self.u.is_subtype(tpe, avoided)
        {
            return None;
        }
        Some(self.ascribed(expr, avoided))
    }

    /// The result of method type `t` after all its clauses.
    pub fn final_result(&mut self, t: TypeId) -> TypeId {
        match *self.u.types().get(t) {
            Type::Method { result, .. } | Type::Poly { result, .. } => self.final_result(result),
            Type::ByName(r) => r,
            _ => t,
        }
    }

    /// `expr: tpe`, the ascription moved into the result of a block (dotty's `ascribeType`).
    fn ascribed(&mut self, expr: TNodeId, tpe: TypeId) -> TNodeId {
        let span = self.tpd.span(expr);
        if let T::Block { stats, expr: last } = *self.tpd.tree(expr)
            && !matches!(self.tpd.tree(last), T::Closure { .. })
        {
            let last = self.ascribed(last, tpe);
            return self.build().block(stats, last, span);
        }
        let tpe = self.simplified(tpe);
        let tpt = self.type_tree(tpe, span.end_pos(), true);
        self.tpd.node(T::Typed { expr, tpt }, span.to_synthetic(), Some(tpe), None)
    }
}
