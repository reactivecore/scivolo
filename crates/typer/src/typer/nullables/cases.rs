// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//! Cases typed against a selector type without `Null` once a case matches `null`, and the
//! selector known not null where its patterns exclude it (dotty's `typedCases`, `matchesNull`,
//! `caseContext`, `afterPatternContext`).

use super::super::{Pt, Typer};
use super::NotNullInfo;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::NodeId;
use syntax::ast::constant::Constant;
use tasty::TypeId;

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
        let tracked = if self.explicit_nulls { self.tracked(sel) } else { None };
        let (mut stripped, mut case_ctx) = (false, ctx);
        let mut typed = Vec::with_capacity(ids.len());
        for (i, &c) in ids.iter().enumerate() {
            let case = self.typed_case_in(c, (Some(sel), selector), pt, bodies.map(|b| b[i]), case_ctx);
            if let Some(r) = tracked
                && let T::CaseDef { pat, .. } = *self.tpd.tree(case)
                && self.is_null_literal(pat)
            {
                case_ctx = self.with_not_null(case_ctx, NotNullInfo::of(vec![r]));
            }
            if self.explicit_nulls && !stripped && self.matches_null(case) {
                selector = self.strip_null(selector);
                stripped = true;
            }
            typed.push(case);
        }
        typed
    }

    /// `ctx` of the guard and body of a case of pattern `pat` against selector `sel`: the selector
    /// known not null if the pattern matches only values that are not (dotty's `caseContext`).
    pub fn case_ctx(&mut self, sel: Option<TNodeId>, pat: TNodeId, ctx: Ctx) -> Ctx {
        let Some(sel) = sel.filter(|_| self.explicit_nulls) else { return ctx };
        match self.tracked(sel) {
            Some(r) if self.matches_not_null(pat) => self.with_not_null(ctx, NotNullInfo::of(vec![r])),
            _ => ctx,
        }
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
