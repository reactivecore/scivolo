// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//! The bounds a GADT approximation replaces a type by (dotty's `fullUpperBound`,
//! `fullLowerBound`).

use super::Typer;
use super::gadt::plain_type_ref;
use tasty::{SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    /// The upper bound for `variance` 1, the lower bound for -1 of `s` or the last parameter it
    /// was made one with, transitively; for the upper bound, a GADT-narrowed type outside those
    /// whose lower bound is that parameter if it conforms to that bound.
    pub fn gadt_bound(&mut self, mut s: SymbolId, variance: i8) -> TypeId {
        let mut seen = vec![s];
        while let Some((lo, hi)) = self.u.gadt_bounds(s)
            && lo == hi
            && let Some(r) = plain_type_ref(self.u, hi).filter(|r| !seen.contains(r))
        {
            seen.push(r);
            s = r;
        }
        let own = self.gadt_ref(s);
        let Some((lo, hi)) = self.current_bounds(s) else { return own };
        if variance < 0 {
            return lo;
        }
        let hi = self.without_bound_above(own, hi);
        let above = self
            .gadt_narrowed()
            .into_iter()
            .find(|&n| !seen.contains(&n) && self.u.gadt_bounds(n).is_some_and(|(l, _)| l == own));
        let above = above.map(|n| self.gadt_ref(n));
        match above {
            Some(a) if is_top(self.u, hi) || self.u.is_subtype(a, hi) => a,
            _ => hi,
        }
    }

    /// Upper bound `hi` of `own` with each pattern-bound type in it whose lower bound `own`
    /// conforms to replaced by the part of its upper bound that is no GADT-constrained type, as
    /// dotty's constraint propagates it.
    fn without_bound_above(&mut self, own: TypeId, hi: TypeId) -> TypeId {
        match *self.u.types().get(hi) {
            Type::And(a, b) => {
                let (a, b) = (self.without_bound_above(own, a), self.without_bound_above(own, b));
                self.u.glb(a, b)
            }
            _ => match plain_type_ref(self.u, hi).filter(|p| self.gadt.bound.contains(p)) {
                Some(p) => match self.current_bounds(p) {
                    Some((lo, up)) if self.u.is_subtype(own, lo) => self.without_gadt_params(up),
                    _ => hi,
                },
                None => hi,
            },
        }
    }

    /// `t` without the parts of an intersection that are GADT-constrained types, `Any` if none
    /// is left.
    fn without_gadt_params(&mut self, t: TypeId) -> TypeId {
        match *self.u.types().get(t) {
            Type::And(a, b) => {
                let (a, b) = (self.without_gadt_params(a), self.without_gadt_params(b));
                self.u.glb(a, b)
            }
            Type::Var(_) => tasty::ANY,
            _ => match plain_type_ref(self.u, t) {
                Some(r) if self.gadt.bound.contains(&r) || self.u.gadt_bounds(r).is_some() => tasty::ANY,
                _ => t,
            },
        }
    }
}

/// Whether `t` is `Any` or a type lambda of result `Any`.
pub fn is_top(u: &mut Universe, t: TypeId) -> bool {
    match *u.types().get(t) {
        Type::Lambda { result, .. } => result == tasty::ANY,
        _ => t == tasty::ANY,
    }
}
