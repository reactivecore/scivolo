// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//! The bounds of a GADT symbol with the symbols it is ordered with (dotty's `fullBounds`).

use crate::tags::CASE;
use crate::{ANY, Kind, SymbolId, TypeId, Universe};

impl Universe {
    /// Gives the bounds of registered `sym` joined with the nearest symbols below it and met with
    /// the nearest above it, leaving out a pattern-bound type below `sym` whose upper bound `sym`
    /// already is above, and one above whose lower bound it is below; its instance twice if it
    /// was made equal to a type (dotty's `fullBounds`, `fullLowerBound`, `fullUpperBound`).
    pub fn gadt_full_bounds(&mut self, sym: SymbolId) -> Option<(TypeId, TypeId)> {
        let e = self.gadt().entry(sym)?;
        if let Some(i) = e.instance {
            return Some((i, i));
        }
        let (mut lo, mut hi) = (e.lo, e.hi);
        let own = self.gadt_ref(sym);
        for p in self.gadt().min_lower(sym) {
            if self.is_pattern_bound(p)
                && self.declared_bounds(p).is_some_and(|(_, h)| self.is_subtype(h, own))
            {
                continue;
            }
            let r = self.gadt_ref(p);
            lo = self.lub(lo, r);
        }
        for p in self.gadt().min_upper(sym) {
            if self.is_pattern_bound(p)
                && self.declared_bounds(p).is_some_and(|(l, _)| self.is_subtype(own, l))
            {
                continue;
            }
            let r = self.gadt_ref(p);
            hi = if hi == ANY { r } else { self.glb(hi, r) };
        }
        Some((lo, hi))
    }

    /// Whether `s` is a type a pattern binds (dotty's `isPatternBound`).
    pub fn is_pattern_bound(&self, s: SymbolId) -> bool {
        let sym = self.symbol(s);
        sym.kind == Kind::Type && sym.flags.has(CASE)
    }

    /// The bounds of `s`'s info.
    fn declared_bounds(&mut self, s: SymbolId) -> Option<(TypeId, TypeId)> {
        let info = self.info(s);
        self.bounds(info)
    }
}
