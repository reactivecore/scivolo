// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Applied type constructors compared by their GADT bounds (dotty's `byGadtBounds` and
//! `byGadtOrdering` in `compareAppliedType1/2`).

use super::Cmp;
use crate::types::TypeId;

impl Cmp<'_> {
    /// Whether type constructor `t1` is below `t2` by the GADT bounds of either or their
    /// ordering in the GADT constraint, and if so whether the symbol whose bound was used is made
    /// equal to a type (dotty's `byGadtBounds` and `byGadtOrdering` in `isMatchingApply`).
    pub fn same_by_gadt(&mut self, t1: TypeId, t2: TypeId) -> Option<bool> {
        if let Some((_, hi)) = self.gadt_bounds(t1)
            && self.recur_frozen(hi, t2)
        {
            return Some(self.gadt_instantiated(t1));
        }
        if let Some((lo, _)) = self.gadt_bounds(t2)
            && self.recur_frozen(t1, lo)
        {
            return Some(self.gadt_instantiated(t2));
        }
        let (p1, p2) = (self.u.gadt_param(t1)?, self.u.gadt_param(t2)?);
        self.u.gadt().is_less(p1, p2).then_some(false)
    }

    /// Whether `t` refers to a registered symbol made equal to a type.
    fn gadt_instantiated(&mut self, t: TypeId) -> bool {
        let Some(p) = self.u.gadt_param(t) else { return false };
        self.u.gadt().entry(p).is_some_and(|e| e.instance.is_some())
    }

    /// `a <: tycon[args]`, `b`, through the GADT bounds of `tycon`, the GADT constraint
    /// frozen (dotty's `byGadtBounds` in `compareAppliedType2`).
    pub fn gadt_applied_below(&mut self, a: TypeId, b: TypeId, tycon: TypeId, args: &[TypeId]) -> bool {
        let Some((lo, hi)) = self.gadt_bounds(tycon) else { return false };
        let lower = self.apply(lo, args);
        self.in_frozen_gadt(|c| match lo == hi {
            true => c.recur_approx(a, lower, false),
            false => c.either(|s| s.fourth_try(a, b), |s| s.recur_approx(a, lower, false)),
        })
    }

    /// `tycon[args] <: b` through the GADT upper bound of `tycon`, the GADT constraint
    /// frozen (dotty's `byGadtBounds` in `compareAppliedType1`).
    pub fn gadt_applied_above(&mut self, b: TypeId, tycon: TypeId, args: &[TypeId]) -> bool {
        let Some((_, hi)) = self.gadt_bounds(tycon) else { return false };
        let upper = self.apply(hi, args);
        self.in_frozen_gadt(|c| c.recur_approx(upper, b, true))
    }
}
