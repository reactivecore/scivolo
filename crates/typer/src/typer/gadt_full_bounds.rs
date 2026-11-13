// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! The bound a GADT approximation replaces a symbol by (dotty's `approximation`).

use super::gadt_cases::Gadt;
use tasty::{SymbolId, Type, TypeId, Universe};

impl Gadt {
    /// Gives the type symbol `s` of the GADT constraint was made equal to, else its full lower
    /// bound for `variance` -1 or where its upper bound refers to it, else its full upper bound
    /// (dotty's `approximation`); `s` itself if it is not in the constraint.
    pub fn bound(&self, u: &mut Universe, s: SymbolId, variance: i8) -> TypeId {
        let Some((lo, hi)) = u.gadt_full_bounds(s) else { return u.gadt_ref(s) };
        let entry_hi = u.gadt().bounds(s).map_or(hi, |(_, h)| h);
        if variance < 0 || refers_at_top(u, entry_hi, s) { lo } else { hi }
    }
}

/// Tells whether `t` or an operand of its unions and intersections refers to `s`, where the GADT
/// constraint holds its references to registered symbols.
fn refers_at_top(u: &mut Universe, t: TypeId, s: SymbolId) -> bool {
    match *u.ty(t) {
        Type::And(a, b) | Type::Or(a, b, _) => refers_at_top(u, a, s) || refers_at_top(u, b, s),
        _ => u.sym_ref(t).is_some_and(|(_, r)| r == s),
    }
}
