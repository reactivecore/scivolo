// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//! Cases typed against a selector type without `Null` once a case matches `null` (dotty's
//! `typedCases`, `matchesNull`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::NodeId;
use syntax::ast::constant::Constant;
use tasty::{SymbolId, TypeId};

impl Typer<'_> {
    /// Cases `ids` against selector `sel` of widened type `selector`, each with the body expected
    /// to be the matching type of `bodies` if given, else `pt`.
    pub fn typed_cases(
        &mut self,
        ids: &[NodeId],
        sel: TNodeId,
        mut selector: TypeId,
        pt: Pt,
        bodies: Option<&[TypeId]>,
        ctx: Ctx,
    ) -> Vec<TNodeId> {
        let tracked = self.nullable_selector(sel);
        let (mut stripped, mut after_null) = (false, false);
        let mut typed = Vec::with_capacity(ids.len());
        for (i, &c) in ids.iter().enumerate() {
            let case = self.typed_case_in(c, selector, pt, bodies.map(|b| b[i]), ctx);
            if let Some(s) = tracked
                && let T::CaseDef { pat, guard, body } = *self.tpd.tree(case)
            {
                if (after_null || self.matches_not_null(pat))
                    && let Some(r) = guard.into_iter().chain([body]).find_map(|t| self.reference_to(t, s))
                {
                    self.unsupported_at(self.tpd.span(r), "selector known not null in a case");
                }
                after_null |= matches!(self.tpd.tree(pat), T::Literal { value: Constant::Null });
            }
            if self.explicit_nulls && !stripped && self.matches_null(case) {
                selector = self.strip_null(selector);
                stripped = true;
            }
            typed.push(case);
        }
        typed
    }

    /// The symbol of selector `sel` under explicit nulls if it is a reference of a type with `Null`
    /// (dotty's `TrackedRef` and `OrNull`, every reference taken as tracked).
    fn nullable_selector(&mut self, sel: TNodeId) -> Option<SymbolId> {
        let s = self.tpd.sym(sel).filter(|_| self.explicit_nulls)?;
        let (T::Ident { name } | T::Select { name, .. }) = *self.tpd.tree(sel) else { return None };
        let tpe = self.tpd.tpe(sel);
        (!name.is_type && self.strip_null(tpe) != tpe).then_some(s)
    }

    fn reference_to(&self, t: TNodeId, s: SymbolId) -> Option<TNodeId> {
        self.tpd.fold(None, t, &mut |found, t| {
            found.or_else(|| {
                (matches!(self.tpd.tree(t), T::Ident { .. } | T::Select { .. }) && self.tpd.sym(t) == Some(s))
                    .then_some(t)
            })
        })
    }

    fn matches_not_null(&self, pat: TNodeId) -> bool {
        match *self.tpd.tree(pat) {
            T::Typed { .. } | T::UnApply { .. } => true,
            T::Alternative { trees } => self.tpd.items(trees).iter().all(|&t| self.matches_not_null(t)),
            _ => false,
        }
    }

    fn matches_null(&self, case: TNodeId) -> bool {
        let T::CaseDef { pat, guard: None, .. } = *self.tpd.tree(case) else { return false };
        self.pattern_matches_null(pat)
    }

    fn pattern_matches_null(&self, pat: TNodeId) -> bool {
        match *self.tpd.tree(pat) {
            T::Literal { value: Constant::Null } => true,
            T::Bind { body, .. } => self.pattern_matches_null(body),
            T::Alternative { trees } => self.tpd.items(trees).iter().any(|&t| self.pattern_matches_null(t)),
            T::Ident { name } => !name.is_type && self.text(name.name) == "_",
            _ => false,
        }
    }
}
