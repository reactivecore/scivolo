// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Types without references to local symbols (dotty's `ensureNoLocalRefs`, `TypeOps.avoid`).

use super::avoid_map::Avoided;
use super::{Pt, Typer};
use crate::tpd::{TNodeId, Tree as T};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether type `t` refers to one of `syms`.
    pub fn refers_to(&mut self, t: TypeId, syms: &[SymbolId]) -> bool {
        let mut found = false;
        let mut classes = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            match *u.types().get(part) {
                Type::Sym(_, s) => found |= syms.contains(&s),
                Type::Constant(tasty::tags::CLASSconst, bits) => classes.push(TypeId(bits as u32)),
                _ => {}
            }
            None
        });
        found || classes.into_iter().any(|c| self.refers_to(c, syms))
    }

    /// `{ defs; tree }`, or `tree` if there are no definitions (dotty's `wrapDefs`), its type
    /// avoiding the definitions (dotty's `avoidingType`); outside the subset if it still refers
    /// to them or is not a supertype of the original.
    pub fn wrap_defs(&mut self, defs: Vec<TNodeId>, tree: TNodeId) -> TNodeId {
        if defs.is_empty() {
            return tree;
        }
        let span = defs.iter().fold(self.tpd.span(tree), |s, &d| s.union(self.tpd.span(d)));
        let span = syntax::ast::Span::synthetic(span.start(), span.end());
        let syms: Vec<SymbolId> = defs.iter().filter_map(|&d| self.tpd.sym(d)).collect();
        let original = self.tpd.get(tree).tpe;
        let tpe = original.map(|t| match self.refers_to(t, &syms) {
            true => match self.avoid_at(t, 1, Avoided::new(&syms, &[])) {
                Some(a) => self.normalized(a.hi()),
                None => t,
            },
            false => t,
        });
        let widened = original.zip(tpe).is_some_and(|(t, a)| t != a && !self.u.is_subtype(t, a));
        if widened || tpe.is_some_and(|t| self.refers_to(t, &syms)) {
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
        let avoided = self.avoid_at(tpe, 1, Avoided::new(locals, opaque))?.hi();
        let mut avoided = self.normalized(avoided);
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
    pub fn ascribed(&mut self, expr: TNodeId, tpe: TypeId) -> TNodeId {
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
