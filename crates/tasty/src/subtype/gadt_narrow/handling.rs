// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! Adding bounds and orderings to the GADT constraint and propagating them (dotty's
//! `ConstraintHandling` run on a `GadtState`, whose `legalBound` is the raw bound).

use super::super::Cmp;
use crate::types::Type;
use crate::{NOTHING, SymbolId, TypeId, Unification};

impl Cmp<'_> {
    /// Adds `bound` to registered `p` and to the symbols above it (a lower bound) or below it
    /// (an upper bound); false for a by-name bound (dotty's `addBoundTransitively`).
    pub fn add_bound_transitively(&mut self, p: SymbolId, bound: TypeId, upper: bool) -> bool {
        if self.has_by_name_operand(bound) {
            return false;
        }
        let others = if upper { self.u.gadt().lower(p).to_vec() } else { self.u.gadt().upper(p).to_vec() };
        self.add_one_bound(p, bound, upper) && others.into_iter().all(|q| self.add_one_bound(q, bound, upper))
    }

    /// Whether `t` or an operand of its unions and intersections is by-name.
    fn has_by_name_operand(&self, t: TypeId) -> bool {
        match *self.u.ty(t) {
            Type::And(a, b) | Type::Or(a, b, _) => self.has_by_name_operand(a) || self.has_by_name_operand(b),
            Type::ByName(_) => true,
            _ => false,
        }
    }

    /// Narrows one bound of registered `p` by `bound`, made an instance where both bounds
    /// become equal; false if the bounds become empty (dotty's `addOneBound`).
    pub fn add_one_bound(&mut self, p: SymbolId, bound: TypeId, upper: bool) -> bool {
        let Some(&crate::GadtEntry { lo, hi, instance: None, .. }) = self.u.gadt().entry(p) else {
            return true;
        };
        let bound = if !upper && self.refers_at_top(bound, p) { self.without_top(bound, p) } else { bound };
        let (lo, hi) = if upper { (lo, self.glb(hi, bound)) } else { (self.lub(lo, bound), hi) };
        if self.try_replace(p, lo, hi) {
            return true;
        }
        let mark = self.u.gadt().mark();
        self.u.gadt_update_entry(p, lo, hi);
        if self.u.gadt().mark() == mark {
            return true;
        }
        let (lo, hi) = self.u.gadt().bounds(p).unwrap_or((lo, hi));
        let sat = self.recur_fresh(lo, hi);
        if sat && let Some((lo, hi)) = self.u.gadt().bounds(p) {
            self.try_replace(p, lo, hi);
        }
        sat
    }

    /// Makes `p` equal to `lo` if both bounds are the same type that is not `p` (dotty's
    /// `tryReplace`).
    fn try_replace(&mut self, p: SymbolId, lo: TypeId, hi: TypeId) -> bool {
        let ok = lo == hi
            && !self.refers_at_top(lo, p)
            && self.u.gadt().entry(p).is_some_and(|e| e.instance.is_none());
        if ok {
            self.u.gadt_replace(p, lo);
        }
        ok
    }

    /// Whether `t` or an operand of its unions and intersections refers to registered `p`.
    fn refers_at_top(&mut self, t: TypeId, p: SymbolId) -> bool {
        match *self.u.ty(t) {
            Type::And(a, b) | Type::Or(a, b, _) => self.refers_at_top(a, p) || self.refers_at_top(b, p),
            _ => self.u.gadt_param(t) == Some(p),
        }
    }

    /// `t` with its operands that refer to `p` replaced by `Nothing` (dotty's `sanitize`).
    fn without_top(&mut self, t: TypeId, p: SymbolId) -> TypeId {
        match *self.u.ty(t) {
            Type::And(a, b) => {
                let (a, b) = (self.without_top(a, p), self.without_top(b, p));
                self.u.intern_type(Type::And(a, b))
            }
            Type::Or(a, b, soft) => {
                let (a, b) = (self.without_top(a, p), self.without_top(b, p));
                self.u.intern_type(Type::Or(a, b, soft))
            }
            _ => match self.u.gadt_param(t) == Some(p) {
                true => NOTHING,
                false => t,
            },
        }
    }

    /// Orders `p1` below `p2`, unifying them if `p2` is below `p1`, and propagates the bounds
    /// (dotty's `addLess`).
    pub fn add_less(&mut self, p1: SymbolId, p2: SymbolId) -> bool {
        if self.u.gadt().is_less(p2, p1) {
            return self.unify(p2, p1);
        }
        let mut down1 = vec![p1];
        down1.extend(self.u.gadt().exclusive_lower(p1, p2));
        let mut up2 = vec![p2];
        up2.extend(self.u.gadt().exclusive_upper(p2, p1));
        let lo1 = self.u.gadt().bounds(p1).map_or(NOTHING, |(lo, _)| lo);
        let hi2 = self.u.gadt().bounds(p2).map_or(crate::ANY, |(_, hi)| hi);
        self.u.gadt_mut().order(p1, p2, Unification::None);
        down1.into_iter().all(|q| self.add_one_bound(q, hi2, true))
            && up2.into_iter().all(|q| self.add_one_bound(q, lo1, false))
    }

    /// Makes `p1` and `p2` one symbol, keeping the one registered at the shallower level and
    /// giving it the bounds of both (dotty's `unify`).
    fn unify(&mut self, p1: SymbolId, p2: SymbolId) -> bool {
        if !self.u.gadt().is_less(p1, p2) {
            self.u.gadt_mut().order(p1, p2, Unification::None);
        }
        let level = |c: &Self, p| c.u.gadt().entry(p).map_or(0, |e| e.level);
        let (kept, removed) = if level(self, p1) <= level(self, p2) { (p1, p2) } else { (p2, p1) };
        let down = self.u.gadt().exclusive_lower(p2, p1);
        let up = self.u.gadt().exclusive_upper(p1, p2);
        let direction = if kept == p1 { Unification::KeepSecond } else { Unification::KeepFirst };
        self.u.gadt_mut().order(p2, p1, direction);

        let (lo1, hi1) = self.u.gadt().bounds(kept).unwrap_or((NOTHING, crate::ANY));
        let (lo2, hi2) = self.u.gadt().bounds(removed).unwrap_or((NOTHING, crate::ANY));
        let (lo, hi) = (self.lub(lo1, lo2), self.glb(hi1, hi2));
        self.u.gadt_update_entry(kept, lo, hi);
        let kept_ref = self.u.gadt_ref(kept);
        self.u.gadt_replace(removed, kept_ref);

        self.recur_fresh(lo, hi)
            && down.into_iter().all(|q| self.add_one_bound(q, hi, true))
            && up.into_iter().all(|q| self.add_one_bound(q, lo, false))
    }
}
