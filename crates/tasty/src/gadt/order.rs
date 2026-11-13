// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/OrderingConstraint.scala
//! The ordering between the symbols of the GADT constraint.

use super::GadtConstraint;
use crate::SymbolId;

/// Which symbol an ordering that unifies two symbols keeps (dotty's `UnificationDirection`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unification {
    None,
    KeepFirst,
    KeepSecond,
}

impl GadtConstraint {
    /// Lists the symbols below `s`, most recently ordered first.
    pub fn lower(&self, s: SymbolId) -> &[SymbolId] {
        self.entry(s).map_or(&[], |e| &e.lower)
    }

    /// Lists the symbols above `s`, most recently ordered first.
    pub fn upper(&self, s: SymbolId) -> &[SymbolId] {
        self.entry(s).map_or(&[], |e| &e.upper)
    }

    /// Tells whether `a` is ordered below `b` (dotty's `isLess`).
    pub fn is_less(&self, a: SymbolId, b: SymbolId) -> bool {
        self.upper(a).contains(&b)
    }

    /// Lists the symbols below `s` that no other symbol below `s` is above (dotty's `minLower`).
    pub fn min_lower(&self, s: SymbolId) -> Vec<SymbolId> {
        let all = self.lower(s);
        all.iter().copied().filter(|&p| !all.iter().any(|&q| self.is_less(p, q))).collect()
    }

    /// Lists the symbols above `s` that no other symbol above `s` is below (dotty's `minUpper`).
    pub fn min_upper(&self, s: SymbolId) -> Vec<SymbolId> {
        let all = self.upper(s);
        all.iter().copied().filter(|&p| !all.iter().any(|&q| self.is_less(q, p))).collect()
    }

    /// Lists the symbols below `s` that are not below `but_not` (dotty's `exclusiveLower`).
    pub fn exclusive_lower(&self, s: SymbolId, but_not: SymbolId) -> Vec<SymbolId> {
        self.lower(s).iter().copied().filter(|&p| !self.is_less(p, but_not)).collect()
    }

    /// Lists the symbols above `s` that are not above `but_not` (dotty's `exclusiveUpper`).
    pub fn exclusive_upper(&self, s: SymbolId, but_not: SymbolId) -> Vec<SymbolId> {
        self.upper(s).iter().copied().filter(|&p| !self.is_less(but_not, p)).collect()
    }

    /// Orders `a` below `b` and everything below `a` below everything above `b`, bounds left
    /// as they are (dotty's `order`); with `direction`, as the step of unifying `a` and `b` that
    /// leaves out the symbol it removes.
    pub fn order(&mut self, a: SymbolId, b: SymbolId, direction: Unification) {
        if a == b || self.is_less(a, b) || !self.contains(a) || !self.contains(b) {
            return;
        }
        let unifying = direction != Unification::None;
        let mut new_upper = self.exclusive_upper(b, a);
        if unifying {
            new_upper.retain(|&p| p != a);
        }
        if !unifying || direction == Unification::KeepSecond {
            new_upper.insert(0, b);
        }
        let mut new_lower = self.exclusive_lower(a, b);
        if unifying {
            new_lower.retain(|&p| p != b);
        }
        if !unifying || direction == Unification::KeepFirst {
            new_lower.insert(0, a);
        }
        for &p in &new_lower {
            self.modify(p, |e| e.upper.splice(0..0, new_upper.iter().copied()).for_each(drop));
        }
        for &p in &new_upper {
            self.modify(p, |e| e.lower.splice(0..0, new_lower.iter().copied()).for_each(drop));
        }
    }
}
