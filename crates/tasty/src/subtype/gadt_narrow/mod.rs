// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//! Narrowing the GADT constraint from inside a comparison (ADR 0119).

mod handling;

use super::{Cmp, NoVars};
use crate::{NOTHING, Registered, SymbolId, TypeId, Universe};

/// Which bounds of an abstract type a comparison was reached through: a bound found that way is
/// imprecise and narrows nothing (dotty's `ApproxState`).
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Approx {
    /// Reached through an upper bound on the left.
    pub low: bool,
    /// Reached through a lower bound on the right.
    pub high: bool,
}

impl Universe {
    /// Registers `syms` at scope nesting `level` and narrows the symbols each is ordered with by
    /// its bounds; false if that empties a bound (dotty's `GadtState.addToConstraint`).
    pub fn gadt_register(&mut self, syms: &[Registered], level: u32) -> bool {
        self.gadt_add(syms, level);
        let mut no_vars = NoVars;
        let mut c = Cmp::new(self, &mut no_vars);
        c.gadt_mode = true;
        syms.iter().all(|r| {
            let Some((lo, hi)) = c.u.gadt().bounds(r.sym) else { return true };
            let (lower, upper) = (c.u.gadt().lower(r.sym).to_vec(), c.u.gadt().upper(r.sym).to_vec());
            lower.into_iter().all(|p| c.add_one_bound(p, hi, true))
                && upper.into_iter().all(|p| c.add_one_bound(p, lo, false))
        })
    }
}

impl Cmp<'_> {
    /// `a <: b` reached through a bound of an abstract type: on the left if `low`, on the right
    /// otherwise (dotty's `isSubType(tp1, tp2, approx.addLow)` and `isSubApproxHi`).
    pub fn recur_approx(&mut self, a: TypeId, b: TypeId, low: bool) -> bool {
        let saved = self.approx;
        if low {
            self.approx.low = true;
        } else {
            self.approx.high = true;
        }
        let result = self.recur(a, b);
        self.approx = saved;
        result
    }

    /// `a <: b` as a comparison of its own, not reached through a bound (dotty's
    /// `isSubType(tp1, tp2)`, whose `ApproxState` is fresh).
    pub fn recur_fresh(&mut self, a: TypeId, b: TypeId) -> bool {
        let saved = std::mem::take(&mut self.approx);
        let result = self.recur(a, b);
        self.approx = saved;
        result
    }

    /// Runs `f` with the GADT constraint frozen (dotty's `inFrozenGadt`).
    pub fn in_frozen_gadt<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let saved = std::mem::replace(&mut self.frozen_gadt, true);
        let result = f(self);
        self.frozen_gadt = saved;
        result
    }

    /// `a <: b` for a type `b` with GADT bounds: through its lower bound, an ordering of the
    /// symbols `a` and `b` refer to, or by narrowing `b` (dotty's `compareGADT` in
    /// `thirdTryNamed`).
    pub fn gadt_above(&mut self, a: TypeId, b: TypeId) -> bool {
        let Some((lo, _)) = self.gadt_bounds(b) else { return false };
        lo != NOTHING && self.recur_frozen(a, lo)
            || self.gadt_ordered(a, b)
            || self.narrow_gadt_bounds(b, a, false)
    }

    /// `a <: b` for a type `a` with GADT bounds: through its upper bound or by narrowing `a`
    /// (dotty's `compareGADT` in `fourthTry`).
    pub fn gadt_below(&mut self, a: TypeId, b: TypeId) -> bool {
        let Some((_, hi)) = self.gadt_bounds(a) else { return false };
        self.recur_frozen(hi, b) || self.narrow_gadt_bounds(a, b, true)
    }

    /// Whether `a` and `b` refer to registered symbols, the one of `a` ordered below `b`'s.
    fn gadt_ordered(&mut self, a: TypeId, b: TypeId) -> bool {
        let (Some(p1), Some(p2)) = (self.u.gadt_param(a), self.u.gadt_param(b)) else { return false };
        self.u.gadt().is_less(p1, p2)
    }

    /// Narrows the GADT bounds of the symbol `t` refers to by `bound`, as upper bound if
    /// `upper`; false if this comparison narrows nothing or the narrowed bounds are empty, the
    /// constraint then as it was (dotty's `narrowGADTBounds`).
    pub fn narrow_gadt_bounds(&mut self, t: TypeId, bound: TypeId, upper: bool) -> bool {
        if !self.gadt_mode || self.frozen_gadt || self.frozen || self.approx != Approx::default() {
            return false;
        }
        let Some(s) = self.u.gadt_param(t) else { return false };
        let dealiased = self.u.dealias(bound);
        if self.u.sym_ref(dealiased).is_some_and(|(_, r)| r == s) {
            return false;
        }
        let mark = self.u.gadt().mark();
        let ok = self.add_gadt_bound(s, bound, upper);
        if !ok {
            self.u.gadt_mut().rollback(mark);
        }
        ok
    }

    /// Adds `bound` as upper bound of registered `s` if `upper`, else as lower bound, as an
    /// ordering if `bound` is another registered symbol (dotty's `GadtState.addBound`).
    fn add_gadt_bound(&mut self, s: SymbolId, bound: TypeId, upper: bool) -> bool {
        let s = match self.gadt_kept(s) {
            Ok(kept) => kept,
            Err(inst) if upper => return self.recur_fresh(inst, bound),
            Err(inst) => return self.recur_fresh(bound, inst),
        };
        let internal = self.internal_gadt(bound);
        let mark = self.u.gadt().mark();
        let result = match self.u.gadt_param(internal) {
            Some(p) if p == s => true,
            Some(p) if upper => self.add_less(s, p),
            Some(p) => self.add_less(p, s),
            None => self.add_bound_transitively(s, internal, upper),
        };
        if self.u.gadt().mark() != mark {
            self.u.gadt_mut().mark_narrowed_since(mark);
            self.u.gadt_mut().set_narrowing();
        }
        result
    }

    /// The registered symbol `s` stands for, through the symbols made equal to others, or the
    /// type it is made equal to (dotty's `stripInternalTypeVar`).
    fn gadt_kept(&mut self, s: SymbolId) -> Result<SymbolId, TypeId> {
        let Some(inst) = self.u.gadt().entry(s).and_then(|e| e.instance) else { return Ok(s) };
        let inst = self.internal_gadt(inst);
        self.u.gadt_param(inst).ok_or(inst)
    }

    /// `t`, or the instance of the registered symbol it is, transitively (dotty's
    /// `stripInternalTypeVar(internalize(t))`).
    fn internal_gadt(&mut self, mut t: TypeId) -> TypeId {
        while let Some(p) = self.u.gadt_param(t)
            && let Some(inst) = self.u.gadt().entry(p).and_then(|e| e.instance)
        {
            t = inst;
        }
        t
    }
}
