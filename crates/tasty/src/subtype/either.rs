// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! dotty's `sufficientEither` and `necessaryEither`: one of two comparisons, keeping the weaker
//! constraint.

use super::Cmp;
use crate::types::{TypeId, VarId};

type Snapshot = Vec<(VarId, (TypeId, TypeId))>;

impl Cmp<'_> {
    /// Whether `first` holds, or else `second`; when both hold, the constraint of `second` is
    /// kept if the one of `first` subsumes it, that of `first` otherwise (dotty's `tryAlso`), or
    /// in a necessary comparison neither unless the one of `second` subsumes it.
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
        let bounds = self.snapshot(&left.vars);
        self.vars.rollback(mark);
        if second(self) {
            let right = self.vars.changes_since(mark).vars;
            let mut vars = right.clone();
            vars.extend(&left.vars);
            if right.is_empty() || vars.iter().all(|&v| self.subsumes_bounds(&bounds, v)) {
                return true;
            }
            if self.necessary {
                let right_bounds = self.snapshot(&right);
                self.vars.rollback(mark);
                self.vars.redo(left);
                if !vars.iter().all(|&v| self.subsumes_bounds(&right_bounds, v)) {
                    self.vars.rollback(mark);
                }
                return true;
            }
        }
        self.vars.rollback(mark);
        self.vars.redo(left);
        true
    }

    /// The bounds of `vars`.
    fn snapshot(&self, vars: &[VarId]) -> Snapshot {
        vars.iter().map(|&v| (v, self.bounds_of(v))).collect()
    }

    /// The bounds of `v`, both its instance if it has one.
    fn bounds_of(&self, v: VarId) -> (TypeId, TypeId) {
        match self.vars.instance(v) {
            Some(t) => (t, t),
            None => self.vars.bounds(v),
        }
    }

    /// Whether the bounds of `v` in `earlier` lie within its current ones (dotty's `subsumes`
    /// for one variable); false for a variable `earlier` does not hold.
    fn subsumes_bounds(&mut self, earlier: &Snapshot, v: VarId) -> bool {
        let (lo2, hi2) = self.bounds_of(v);
        let Some(&(_, (lo1, hi1))) = earlier.iter().find(|(w, _)| *w == v) else { return false };
        self.recur_frozen(lo2, lo1) && self.recur_frozen(hi1, hi2)
    }
}
