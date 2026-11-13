// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! dotty's `sufficientEither` and `necessaryEither`: one of two comparisons, keeping the weaker
//! constraint.

use super::{Changes, Cmp};
use crate::gadt::GadtChanges;
use crate::types::{TypeId, VarId};
use crate::{Mark, SymbolId};

type Snapshot = Vec<(VarId, (TypeId, TypeId))>;

/// The constraints a branch of `necessary_either` ended with.
struct Branch {
    vars: Vec<VarId>,
    bounds: Snapshot,
    gadt: Option<GadtChanges>,
}

impl crate::Universe {
    /// Whether GADT state `strong` subsumes `weak` on the symbols `pre`, `None` standing for the
    /// state both started from (dotty's `subsumes` on GADT constraints).
    pub fn gadt_subsumes(
        &mut self,
        strong: Option<&GadtChanges>,
        weak: Option<&GadtChanges>,
        pre: &[SymbolId],
    ) -> bool {
        Cmp::new(self, &mut super::NoVars).gadt_subsumes(strong, weak, pre)
    }
}

impl Cmp<'_> {
    /// Whether `first` holds, or else `second`; when both hold, the constraint of `second` is
    /// kept if the one of `first` subsumes it, that of `first` otherwise (dotty's `tryAlso`), or
    /// in a necessary comparison the subsumed one of both constraints and GADT constraints,
    /// else neither.
    pub fn either(
        &mut self,
        first: impl FnOnce(&mut Self) -> bool,
        second: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        if self.necessary {
            return self.necessary_either(first, second);
        }
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
        }
        self.vars.rollback(mark);
        self.vars.redo(left);
        true
    }

    /// `either` keeping only constraints that both branches need (dotty's `necessaryEither`).
    fn necessary_either(
        &mut self,
        first: impl FnOnce(&mut Self) -> bool,
        second: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        let (mark, gadt_mark) = (self.vars.mark(), self.u.gadt().mark());
        if !first(self) {
            self.vars.rollback(mark);
            return second(self);
        }
        let changes = self.vars.changes_since(mark);
        let gadt = self.u.gadt().changes_since(gadt_mark);
        if changes.vars.is_empty() && gadt.is_none() {
            return true;
        }
        let left = Branch { vars: changes.vars.clone(), bounds: self.snapshot(&changes.vars), gadt };
        self.restore(mark, gadt_mark, None);
        let pre: Vec<SymbolId> = self.u.gadt().symbols().collect();
        if !second(self) {
            self.restore(mark, gadt_mark, Some((changes, left.gadt)));
            return true;
        }
        let vars = self.vars.changes_since(mark).vars;
        let right =
            Branch { bounds: self.snapshot(&vars), vars, gadt: self.u.gadt().changes_since(gadt_mark) };
        if self.subsumes(&left, &right, &pre) {
            return true;
        }
        self.restore(mark, gadt_mark, Some((changes, left.gadt.clone())));
        if !self.subsumes(&right, &left, &pre) {
            self.restore(mark, gadt_mark, None);
        }
        true
    }

    /// Rolls the constraint and the GADT constraint back to the marks, then makes `changes`
    /// again.
    fn restore(&mut self, mark: Mark, gadt_mark: Mark, changes: Option<(Changes, Option<GadtChanges>)>) {
        self.vars.rollback(mark);
        self.u.gadt_mut().rollback(gadt_mark);
        let Some((changes, gadt)) = changes else { return };
        self.vars.redo(changes);
        if let Some(g) = gadt {
            self.u.gadt_mut().redo(g);
        }
    }

    /// Whether the constraints of `strong` subsume those of `weak`, `weak` being current: each
    /// variable's bounds within the other's, each GADT symbol of `pre` ordered and bounded at
    /// least as in `weak` (dotty's `subsumes` on both).
    fn subsumes(&mut self, strong: &Branch, weak: &Branch, pre: &[SymbolId]) -> bool {
        let mut vars = weak.vars.clone();
        vars.extend(&strong.vars);
        let vars_ok = weak.vars.is_empty() || vars.iter().all(|&v| self.subsumes_bounds(&strong.bounds, v));
        vars_ok && self.gadt_subsumes(strong.gadt.as_ref(), weak.gadt.as_ref(), pre)
    }

    /// Whether GADT state `strong` subsumes `weak`, `None` standing for the state before both.
    fn gadt_subsumes(
        &mut self,
        strong: Option<&GadtChanges>,
        weak: Option<&GadtChanges>,
        pre: &[SymbolId],
    ) -> bool {
        let Some(weak) = weak else { return true };
        let Some(strong) = strong else { return false };
        pre.iter().all(|&p| {
            let (Some(s), Some(w)) = (strong.entry(p).cloned(), weak.entry(p).cloned()) else { return false };
            let bounds = |e: &crate::GadtEntry| e.instance.map_or((e.lo, e.hi), |i| (i, i));
            let ((slo, shi), (wlo, whi)) = (bounds(&s), bounds(&w));
            w.upper.iter().all(|q| s.upper.contains(q))
                && self.recur_frozen(wlo, slo)
                && self.recur_frozen(shi, whi)
        })
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
