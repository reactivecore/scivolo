// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/OrderingConstraint.scala
//! Registering symbols in the GADT constraint and changing their entries.

use super::{Entry, Unification};
use crate::types::Type;
use crate::{ANY, NOTHING, SymbolId, TypeId, Universe};

/// A symbol to register with its declared bounds.
#[derive(Clone, Copy, Debug)]
pub struct Registered {
    pub sym: SymbolId,
    pub lo: TypeId,
    pub hi: TypeId,
}

impl Universe {
    /// Registers `syms` at scope nesting `level`, a bound that is another registered symbol
    /// taken as an ordering (dotty's `OrderingConstraint.add` and `init`).
    pub fn gadt_add(&mut self, syms: &[Registered], level: u32) {
        for r in syms {
            let (lower, upper) = (Vec::new(), Vec::new());
            let e = Entry { lo: r.lo, hi: r.hi, instance: None, lower, upper, level, narrowed: false };
            self.gadt_mut().set_entry(r.sym, e);
        }
        for r in syms {
            let lower = self.gadt_param(r.lo);
            let upper = self.gadt_param(r.hi);
            self.gadt_mut().modify(r.sym, |e| {
                e.lo = if lower.is_some() { NOTHING } else { e.lo };
                e.hi = if upper.is_some() { ANY } else { e.hi };
            });
            if let Some(p) = upper {
                self.gadt_mut().order(r.sym, p, Unification::None);
            }
            if let Some(p) = lower {
                self.gadt_mut().order(p, r.sym, Unification::None);
            }
            let mut nested = Vec::new();
            self.gadt_nested_refs(r.lo, false, &mut nested);
            self.gadt_nested_refs(r.hi, true, &mut nested);
            for p in nested {
                self.gadt_mut().add_nested(p);
            }
        }
    }

    /// Collects the registered symbols `t` refers to inside its intersections, or its unions if
    /// `upper` (dotty's `substDependentSyms` below the top level).
    fn gadt_nested_refs(&mut self, t: TypeId, upper: bool, out: &mut Vec<SymbolId>) {
        let (a, b) = match *self.ty(t) {
            Type::And(a, b) if !upper => (a, b),
            Type::Or(a, b, _) if upper => (a, b),
            _ => return,
        };
        for x in [a, b] {
            match self.gadt_param(x) {
                Some(p) => out.push(p),
                None => self.gadt_nested_refs(x, upper, out),
            }
        }
    }

    /// Sets the bounds of registered `s` to `lo` and `hi` and orders it above the symbols `lo`
    /// depends on and below those `hi` does (dotty's `updateEntry`).
    pub fn gadt_update_entry(&mut self, s: SymbolId, lo: TypeId, hi: TypeId) {
        self.gadt_mut().modify(s, |e| (e.lo, e.hi, e.instance) = (lo, hi, None));
        for p in self.gadt_dependent_params(lo, false) {
            self.gadt_mut().order(p, s, Unification::None);
        }
        for p in self.gadt_dependent_params(hi, true) {
            self.gadt_mut().order(s, p, Unification::None);
        }
    }

    /// Lists the symbols a fresh symbol below (`upper`) or above `t` is ordered with: `t`
    /// itself if registered, with those above or below it (dotty's `dependentParams`).
    fn gadt_dependent_params(&mut self, t: TypeId, upper: bool) -> Vec<SymbolId> {
        let Some(p) = self.gadt_param(t) else { return Vec::new() };
        let g = self.gadt();
        std::iter::once(p).chain(if upper { g.upper(p) } else { g.lower(p) }.iter().copied()).collect()
    }

    /// Makes registered `s` equal to `t`, drops it from the ordering and replaces it in the
    /// entries that are `s` (dotty's `OrderingConstraint.replace`).
    pub fn gadt_replace(&mut self, s: SymbolId, t: TypeId) {
        let t = self.dealias(t);
        if !self.gadt().contains(s) || self.gadt_param(t) == Some(s) {
            return;
        }
        let kept = self.gadt_param(t);
        if self.gadt().nested.iter().any(|&n| n == s || Some(n) == kept) {
            self.gadt_mut().set_inexact();
        }
        self.gadt_mut().modify(s, |e| e.instance = Some(t));
        let (lower, upper) = (self.gadt().lower(s).to_vec(), self.gadt().upper(s).to_vec());
        for p in lower {
            self.gadt_mut().modify(p, |e| e.upper.retain(|&q| q != s));
        }
        for p in upper {
            self.gadt_mut().modify(p, |e| e.lower.retain(|&q| q != s));
        }
        self.gadt_mut().modify(s, |e| (e.lower, e.upper) = (Vec::new(), Vec::new()));
        let others: Vec<_> = self.gadt().symbols().filter(|&o| o != s).collect();
        for o in others {
            let Some(e) = self.gadt().entry(o).cloned() else { continue };
            let sub = |u: &mut Self, x: TypeId| if u.gadt_param(x) == Some(s) { t } else { x };
            let instance = e.instance.map(|i| sub(self, i));
            let (lo, hi) = (sub(self, e.lo), sub(self, e.hi));
            self.gadt_mut().modify(o, |e| (e.lo, e.hi, e.instance) = (lo, hi, instance));
        }
    }
}
