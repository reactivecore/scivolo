// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Two applications of the same type alias compared by their arguments or their expansions.

use super::Cmp;
use crate::types::TypeId;

impl Cmp<'_> {
    /// `a <: tycon2[args2]` for `a` an application of the same alias as `tycon2` to `args1`: by
    /// the arguments, also by both expansions, keeping the weaker constraint, GADT constraints
    /// frozen (dotty's `isMatchingApply` with `tryAlso`).
    pub fn same_alias_args(
        &mut self,
        a: TypeId,
        (tycon2, args2): (TypeId, &[TypeId]),
        (args1, variances): (&[TypeId], &[i8]),
    ) -> bool {
        self.in_frozen_gadt(|c| {
            c.either(
                |c| c.sub_args(args1, args2, variances, &[]),
                |c| {
                    let b = c.u.intern_applied(tycon2, args2);
                    let (da, db) = (c.u.dealias(a), c.u.dealias(b));
                    (da != a || db != b) && c.recur(da, db)
                },
            )
        })
    }
}
