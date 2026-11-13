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

#[cfg(test)]
mod tests {
    use super::Gadt;
    use tasty::{ANY, Flags, Kind, NOTHING, Registered, Symbol, SymbolId, Type, TypeId, Universe};

    fn define(u: &mut Universe, name: &str, kind: Kind) -> SymbolId {
        let root = u.root();
        let s = u.define(Symbol {
            name: name.into(),
            kind,
            owner: Some(root),
            flags: Flags::default(),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        let unbounded = u.intern_type(Type::Bounds(NOTHING, ANY));
        u.set_info(s, unbounded);
        s
    }

    /// Registers `T` with upper bound `hi(T, X)` and lower bound `X`, for a class `X`.
    fn bound_of(
        hi: impl Fn(&mut Universe, TypeId, TypeId) -> TypeId,
        variance: i8,
    ) -> (TypeId, TypeId, TypeId) {
        let mut u = Universe::empty();
        let (t, x) = (define(&mut u, "T", Kind::TypeParam), define(&mut u, "X", Kind::Class));
        let [t_ref, x_ref] = [t, x].map(|s| u.intern_type(Type::Sym(None, s)));
        let hi = hi(&mut u, t_ref, x_ref);
        u.gadt_add(&[Registered { sym: t, lo: x_ref, hi }], 0);
        (Gadt::default().bound(&mut u, t, variance), x_ref, hi)
    }

    #[test]
    fn approximations_take_the_lower_bound_where_dotty_s_occurs_in_finds_the_symbol() {
        let (bound, _, hi) = bound_of(|u, t, x| u.intern_applied(x, &[t]), 1);
        assert_eq!(bound, hi, "T <: X[T] takes the upper bound");
        let (bound, lo, _) = bound_of(|u, t, x| u.intern_type(Type::Or(x, t, false)), 1);
        assert_eq!(bound, lo, "T <: X | T takes the lower bound");
        let (bound, lo, _) = bound_of(|_, _, x| x, -1);
        assert_eq!(bound, lo, "contravariant");
        let (bound, _, hi) = bound_of(|_, _, x| x, 1);
        assert_eq!(bound, hi, "covariant");
    }

    #[test]
    fn approximations_of_unregistered_symbols_are_the_symbols() {
        let mut u = Universe::empty();
        let t = define(&mut u, "T", Kind::TypeParam);
        let t_ref = u.intern_type(Type::Sym(None, t));
        assert_eq!(Gadt::default().bound(&mut u, t, 1), t_ref);
    }
}
