//! dotty's `sufficientEither`: one of two comparisons, keeping the weaker constraint.

use super::Cmp;
use crate::types::{TypeId, VarId};

impl Cmp<'_> {
    /// Whether `first` holds, or else `second`; when both hold, the constraint of `second` is
    /// kept if the one of `first` subsumes it, that of `first` otherwise (dotty's `tryAlso`).
    pub fn either(&mut self, first: impl Fn(&mut Self) -> bool, second: impl Fn(&mut Self) -> bool) -> bool {
        let mark = self.vars.mark();
        if !first(self) {
            self.vars.rollback(mark);
            return second(self);
        }
        if self.vars.mark() == mark {
            return true;
        }
        let changed = self.vars.changed_since(mark);
        let left: Vec<(VarId, (TypeId, TypeId))> = changed.iter().map(|&v| (v, self.bounds_of(v))).collect();
        self.vars.rollback(mark);
        if second(self) {
            let mut vars = self.vars.changed_since(mark);
            vars.extend(changed.iter().copied());
            if vars.iter().all(|&v| self.subsumes_bounds(&left, v)) {
                return true;
            }
        }
        self.vars.rollback(mark);
        first(self)
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
