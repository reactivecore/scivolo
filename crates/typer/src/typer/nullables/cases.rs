// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//! Cases typed against a selector type without `Null` once a case matches `null`, and the
//! selector known not null where its patterns exclude it (dotty's `typedCases`, `matchesNull`,
//! `caseContext`, `afterPatternContext`).

use super::super::{Pt, Typer};
use super::conditionals::is_null_literal;
use super::{NotNullInfo, flow};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use syntax::ast::NodeId;
use syntax::ast::constant::Constant;
use syntax::names::Names;
use tasty::TypeId;

impl Typer<'_> {
    /// Types cases `ids` against selector `sel` of widened type `selector`, each with the body
    /// expected to be the matching type of `bodies` if given, else `pt`. Under explicit nulls the
    /// cases after `case null =>` know `sel` not null, and those after a case that matches `null`
    /// match against `selector` without `Null`.
    pub fn typed_cases(
        &mut self,
        ids: &[NodeId],
        sel: TNodeId,
        mut selector: TypeId,
        pt: Pt,
        bodies: Option<&[TypeId]>,
        ctx: Ctx,
    ) -> Vec<TNodeId> {
        let tracked = if self.explicit_nulls {
            let (nullables, mut f) = flow(self);
            nullables.tracked(&mut f, sel)
        } else {
            None
        };
        let (mut stripped, mut case_ctx) = (false, ctx);
        let mut typed = Vec::with_capacity(ids.len());
        for (i, &c) in ids.iter().enumerate() {
            let case = self.typed_case_in(c, (Some(sel), selector), pt, bodies.map(|b| b[i]), case_ctx);
            if let Some(r) = tracked
                && let T::CaseDef { pat, .. } = *self.tpd.tree(case)
                && is_null_literal(&self.tpd, pat)
            {
                case_ctx = self.nullables.with_not_null(case_ctx, NotNullInfo::of(vec![r]));
            }
            if self.explicit_nulls && !stripped && matches_null(&self.tpd, self.names, case) {
                selector = self.strip_null(selector);
                stripped = true;
            }
            typed.push(case);
        }
        typed
    }

    /// Gives `ctx` of the guard and body of a case of pattern `pat` against selector `sel` the
    /// selector known not null if the pattern matches only values that are not, as
    /// `case s: String` (dotty's `caseContext`).
    pub fn case_ctx(&mut self, sel: Option<TNodeId>, pat: TNodeId, ctx: Ctx) -> Ctx {
        let Some(sel) = sel.filter(|_| self.explicit_nulls) else { return ctx };
        let (nullables, mut f) = flow(self);
        match nullables.tracked(&mut f, sel) {
            Some(r) if matches_not_null(f.tpd, pat) => nullables.with_not_null(ctx, NotNullInfo::of(vec![r])),
            _ => ctx,
        }
    }
}

fn matches_not_null(tpd: &Tpd, pat: TNodeId) -> bool {
    match *tpd.tree(pat) {
        T::Typed { .. } | T::UnApply { .. } => true,
        T::Alternative { trees } => tpd.items(trees).iter().all(|&t| matches_not_null(tpd, t)),
        _ => false,
    }
}

fn matches_null(tpd: &Tpd, names: &Names, case: TNodeId) -> bool {
    let T::CaseDef { pat, guard: None, .. } = *tpd.tree(case) else { return false };
    pattern_matches_null(tpd, names, pat)
}

fn pattern_matches_null(tpd: &Tpd, names: &Names, pat: TNodeId) -> bool {
    match *tpd.tree(pat) {
        T::Literal { value: Constant::Null } => true,
        T::Bind { body, .. } => pattern_matches_null(tpd, names, body),
        T::Alternative { trees } => tpd.items(trees).iter().any(|&t| pattern_matches_null(tpd, names, t)),
        T::Ident { name } => !name.is_type && names.text(name.name) == "_",
        _ => false,
    }
}
