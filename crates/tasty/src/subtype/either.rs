// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! dotty's `sufficientEither`: one of two comparisons, keeping the weaker constraint.

use super::Cmp;
use crate::types::{TypeId, VarId};

impl Cmp<'_> {
    /// Whether `first` holds, or else `second`; when both hold, the constraint of `second` is
    /// kept if the one of `first` subsumes it, that of `first` otherwise (dotty's `tryAlso`).
    pub fn either(
        &mut self,
        first: impl FnOnce(&mut Self) -> bool,
        second: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        let mark = self.vars.mark();
        if !first(self) {
            self.vars.rollback(mark);
            return second(self);
        }
        let left = self.vars.changes_since(mark);
        if left.vars.is_empty() {
            return true;
        }
        let bounds: Vec<(VarId, (TypeId, TypeId))> =
            left.vars.iter().map(|&v| (v, self.bounds_of(v))).collect();
        self.vars.rollback(mark);
        if second(self) {
            let mut vars = self.vars.changes_since(mark).vars;
            vars.extend(&left.vars);
            if vars.iter().all(|&v| self.subsumes_bounds(&bounds, v)) {
                return true;
            }
        }
        self.vars.rollback(mark);
        self.vars.redo(left);
        true
    }

    /// The bounds of `v`, both its instance if it has one.
    fn bounds_of(&self, v: VarId) -> (TypeId, TypeId) {
        match self.vars.instance(v) {
            Some(t) => (t, t),
            None => self.vars.bounds(v),
        }
    }

    /// Whether the bounds of `v` in `left` lie within its current ones (dotty's `subsumes` for
    /// one variable); false for a variable `left` leaves as it was.
    fn subsumes_bounds(&mut self, left: &[(VarId, (TypeId, TypeId))], v: VarId) -> bool {
        let (lo2, hi2) = self.bounds_of(v);
        let Some(&(_, (lo1, hi1))) = left.iter().find(|(w, _)| *w == v) else { return false };
        self.recur_frozen(lo2, lo1) && self.recur_frozen(hi1, hi2)
    }
}
