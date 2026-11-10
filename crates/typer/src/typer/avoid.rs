// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Types without references to local symbols (dotty's `ensureNoLocalRefs`, `TypeOps.avoid`).

use super::type_ops::TypeOps;
use super::{Pt, Typer};
use crate::tpd::{TNodeId, Tree as T};
use tasty::{SymbolId, Type, TypeId};

/// One approximation of a type by types without references to local symbols (dotty's
/// `AvoidMap`): the symbols to avoid, those whose bounds are unknown, the local symbols being
/// expanded and the types whose bounds are; reaching one of the last three fails. A reference to
/// `own`, the type member whose declared bounds are traversed, gives `Nothing .. Any` (dotty's
/// `LazyRef` of an F-bound under `isExpandingBounds`). In the bound of a type variable, type
/// variables stay as they are (dotty's `TypeParamRef`s of a constraint), but not in the info of
/// a local symbol. A nested part is approximated by a copy with other fields.
#[derive(Clone, Copy)]
pub struct Avoid<'a> {
    pub locals: &'a [SymbolId],
    pub opaque: &'a [SymbolId],
    pub visiting: &'a [SymbolId],
    pub expanding: &'a [SymbolId],
    pub own: Option<SymbolId>,
    pub in_var_bound: bool,
}

impl<'a> Avoid<'a> {
    /// Starts an approximation that avoids `locals` and fails on a reference to `opaque`.
    pub fn new(locals: &'a [SymbolId], opaque: &'a [SymbolId]) -> Self {
        Avoid { locals, opaque, visiting: &[], expanding: &[], own: None, in_var_bound: false }
    }

    /// Copies `self` for a type that is not part of the bounds of `own`.
    pub fn elsewhere(self) -> Self {
        Avoid { own: None, ..self }
    }
}

impl TypeOps<'_> {
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
}

impl Typer<'_> {
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
        let tpe = original.map(|t| {
            let avoided = Avoid::new(&syms, &[]).at(self, t, 1);
            match avoided {
                Some(a) if self.refers_to(t, &syms) => self.normalized(a.hi()),
                _ => t,
            }
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
        if !self.types().escapes(tpe, locals) && !self.types().var_bounds_refer_to(tpe, locals) {
            return Some(expr);
        }
        let avoided = Avoid::new(locals, opaque).at(self, tpe, 1)?.hi();
        let mut avoided = self.normalized(avoided);
        if let Pt::Type(p) = pt
            && let p = self.zonk(p)
            && self.is_fully_defined(p)
            && !self.u.is_subtype(avoided, p)
        {
            avoided = p;
        }
        let checked = self.without_rec_refinements(avoided);
        if self.refers_to(avoided, locals)
            || self.types().var_bounds_refer_to(avoided, locals)
            || !self.u.is_subtype(tpe, checked)
        {
            return None;
        }
        Some(self.ascribed(expr, avoided))
    }
}
