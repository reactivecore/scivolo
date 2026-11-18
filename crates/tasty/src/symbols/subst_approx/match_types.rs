// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Match types under the approximating substitution.

use super::Subst;
use crate::approx::{self, Approx};
use crate::symbols::Universe;
use crate::types::{TypeId, TypeList};

impl Universe {
    /// A match type mapped (dotty's `derivedMatchType`): the range over its bound's range, the
    /// bounds of its bound for a scrutinee that stays open, else the range over its cases.
    pub fn approximated_match(
        &mut self,
        bound: TypeId,
        scrutinee: TypeId,
        cases: TypeList,
        v: i8,
        s: Subst<'_>,
    ) -> Approx {
        let mapped_bound = self.approximated(bound, v, s);
        let scrutinee = self.approximated(scrutinee, 0, s);
        let cases = self.types_of(cases).to_vec();
        let cases: Vec<_> = cases.iter().map(|&c| self.approximated(c, v, s)).collect();
        let lows: Vec<_> = cases.iter().map(|c| c.lo()).collect();
        let highs: Vec<_> = cases.iter().map(|c| c.hi()).collect();

        let Approx::Exact(b) = mapped_bound else {
            let lo = self.intern_match(mapped_bound.lo(), scrutinee.hi(), &highs);
            let hi = self.intern_match(mapped_bound.hi(), scrutinee.hi(), &highs);
            return approx::range(self, Approx::Exact(lo), Approx::Exact(hi), v);
        };
        if scrutinee.is_range() {
            let (lo, hi) = self.bounds(b).unwrap_or((b, b));
            return approx::range(self, Approx::Exact(lo), Approx::Exact(hi), v);
        }

        let lo = self.intern_match(b, scrutinee.hi(), &lows);
        let hi = self.intern_match(b, scrutinee.hi(), &highs);
        approx::range(self, Approx::Exact(lo), Approx::Exact(hi), v)
    }
}
