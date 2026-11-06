// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/VarianceChecker.scala

use super::Typer;
use super::variance_types::{Checked, Misplaced, label, of_sign};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::Type;

impl Typer<'_> {
    /// Checks that the parameters of the type lambdas type definition `t` defines with declared
    /// variances occur only at those (dotty's `checkLambda`).
    pub fn check_lambda_variances(&mut self, t: TNodeId, errors: &mut Vec<(Span, String)>) {
        let (Some(sym), T::TypeDef { rhs, .. }) = (self.tpd.sym(t), self.tpd.tree(t).clone()) else { return };
        if !matches!(self.tpd.tree(rhs), T::LambdaTypeTree { .. }) {
            return;
        }
        let info = self.u.info(sym);
        let tops = match *self.u.types().get(info) {
            Type::Alias(a) => vec![a],
            Type::Bounds(lo, hi) => vec![lo, hi],
            _ => vec![info],
        };
        for top in tops {
            let Type::Lambda { binder, params, result } = self.u.types().get(top).clone() else { continue };
            let Some(variances) = self.u.declared_variances(binder) else { continue };
            let parts = match *self.u.types().get(result) {
                Type::Bounds(lo, hi) => vec![lo, hi],
                _ => vec![result],
            };
            let checked = Checked::Lambda(binder, &variances);
            if let Some(Misplaced::Lambda(i, v)) =
                parts.into_iter().find_map(|p| self.misplaced(checked, p, 1, 0))
            {
                let shown = self.u.show_user(top);
                let shown = shown.split_once("=>> ").map_or(shown.as_str(), |(_, r)| r);
                let (own, at, name) = (label(of_sign(variances[i])), label(v), &params[i].0);
                let msg = format!("{own} type parameter {name} occurs in {at} position in {shown}");
                errors.push((self.tpd.span(t), msg));
                return;
            }
        }
    }
}
