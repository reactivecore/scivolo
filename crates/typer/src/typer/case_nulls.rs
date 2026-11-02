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
use tasty::TypeId;

impl Typer<'_> {
    /// Cases `ids` against a selector of type `selector`, each with the body expected to be the
    /// matching type of `bodies` if given, else `pt`.
    pub fn typed_cases(
        &mut self,
        ids: &[NodeId],
        mut selector: TypeId,
        pt: Pt,
        bodies: Option<&[TypeId]>,
        ctx: Ctx,
    ) -> Vec<TNodeId> {
        let mut stripped = false;
        let mut typed = Vec::with_capacity(ids.len());
        for (i, &c) in ids.iter().enumerate() {
            let case = self.typed_case_in(c, selector, pt, bodies.map(|b| b[i]), ctx);
            if self.explicit_nulls && !stripped && self.matches_null(case) {
                selector = self.strip_null(selector);
                stripped = true;
            }
            typed.push(case);
        }
        typed
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
