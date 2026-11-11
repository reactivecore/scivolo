// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/VarianceChecker.scala

use super::super::variance_ops::{Checked, Misplaced, VarianceOps, label, of_sign};
use super::VarianceCheck;
use crate::tpd::{TNodeId, Tree as T};
use tasty::Type;

impl VarianceCheck<'_> {
    /// Checks that the parameters of the type lambdas type definition `t` defines with declared
    /// variances occur only at those (dotty's `checkLambda`), as `X` does not in
    /// `type F[+X] = X => Int`.
    pub fn check_lambda_variances(&mut self, ops: &mut VarianceOps, t: TNodeId) {
        let (Some(sym), T::TypeDef { rhs, .. }) = (self.tpd.sym(t), self.tpd.tree(t).clone()) else { return };
        let T::LambdaTypeTree { tparams, .. } = *self.tpd.tree(rhs) else { return };
        let info = ops.u.info(sym);
        let mut tops = match *ops.u.ty(info) {
            Type::Alias(a) => vec![a],
            Type::Bounds(lo, hi) => vec![lo, hi],
            _ => vec![info],
        };
        tops.extend(ops.u.opaque_alias(sym));
        for top in tops {
            let Some((binder, params, result)) =
                ops.u.lambda(top).map(|l| (l.binder, l.params.to_vec(), l.result))
            else {
                continue;
            };
            let Some(variances) = ops.u.declared_variances(binder) else { continue };
            let parts = match ops.u.bounds(result) {
                Some((lo, hi)) => vec![lo, hi],
                None => vec![result],
            };
            let checked = Checked::Lambda(binder, &variances);
            if let Some(Misplaced::Lambda(i, v)) =
                parts.into_iter().find_map(|p| ops.misplaced(checked, p, 1, 0))
            {
                let named = ops.u.map_type(result, &mut |u, part| {
                    let (_, k) = u.param_ref(part).filter(|&(b, _)| b == binder)?;
                    Some(u.intern_type(Type::Local(binder, params[k as usize].0)))
                });
                let shown = ops.u.show_user(named);
                let (own, at, name) = (label(of_sign(variances[i])), label(v), ops.u.name(params[i].0));
                let msg = format!("{own} type parameter {name} occurs in {at} position in {shown}");
                let tparam = self.tpd.items(tparams).get(i).copied();
                self.errors.push((self.tpd.span(tparam.unwrap_or(t)), msg));
                return;
            }
        }
    }
}
