//! The GADT constraint's ordering of symbols (dotty's `OrderingConstraint` under a
//! `GadtConstraint`).

mod support;

use support::define;
use tasty::{ANY, Kind, NOTHING, Registered, SymbolId, Type, TypeId, Unification, Universe};

/// Defines unbounded type parameters `names` and a class `X` in an empty universe.
fn setup<const N: usize>(names: [&str; N]) -> (Universe, [SymbolId; N], TypeId) {
    let mut u = Universe::empty();
    let root = u.root();
    let unbounded = u.intern_type(Type::Bounds(NOTHING, ANY));
    let syms = names.map(|n| define(&mut u, n, Kind::TypeParam, root, unbounded));
    let x = define(&mut u, "X", Kind::Class, root, unbounded);
    let x = u.intern_type(Type::Sym(None, x));
    (u, syms, x)
}

fn unbounded(syms: &[SymbolId]) -> Vec<Registered> {
    syms.iter().map(|&sym| Registered { sym, lo: NOTHING, hi: ANY }).collect()
}

fn type_ref(u: &mut Universe, s: SymbolId) -> TypeId {
    u.intern_type(Type::Sym(None, s))
}

#[test]
fn ordering_is_transitive_with_the_latest_first() {
    let (mut u, [a, b, c], _) = setup(["A", "B", "C"]);
    u.gadt_add(&unbounded(&[a, b, c]), 0);
    u.gadt_mut().order(a, b, Unification::None);
    u.gadt_mut().order(b, c, Unification::None);
    let g = u.gadt();
    assert_eq!(g.upper(a), [c, b]);
    assert_eq!(g.lower(c), [b, a]);
    assert!(g.is_less(a, c) && !g.is_less(c, a));
    assert_eq!(g.min_upper(a), [b]);
    assert_eq!(g.min_lower(c), [b]);
    assert_eq!(g.exclusive_upper(a, b), [b]);
    assert_eq!(g.exclusive_lower(c, b), [b]);
}

#[test]
fn a_bound_that_is_a_registered_symbol_becomes_an_ordering() {
    let (mut u, [a, b, c], x) = setup(["A", "B", "C"]);
    let a_ref = type_ref(&mut u, a);
    u.gadt_add(
        &[Registered { sym: a, lo: NOTHING, hi: x }, Registered { sym: b, lo: NOTHING, hi: a_ref }],
        0,
    );
    assert_eq!(u.gadt().bounds(b), Some((NOTHING, ANY)));
    assert!(u.gadt().is_less(b, a));
    assert_eq!(u.gadt().bounds(a), Some((NOTHING, x)));

    u.gadt_add(&unbounded(&[c]), 1);
    let b_ref = type_ref(&mut u, b);
    u.gadt_update_entry(c, b_ref, ANY);
    assert!(u.gadt().is_less(b, c) && !u.gadt().is_less(a, c));
    assert_eq!(u.gadt().lower(c), [b]);
    assert_eq!(u.gadt().bounds(c), Some((b_ref, ANY)));
}

#[test]
fn replace_instantiates_and_keeps_the_ordering_around() {
    let (mut u, [a, b, c], x) = setup(["A", "B", "C"]);
    u.gadt_add(&unbounded(&[a, b, c]), 0);
    u.gadt_mut().order(a, b, Unification::None);
    u.gadt_mut().order(b, c, Unification::None);
    u.gadt_replace(b, x);
    let g = u.gadt();
    assert_eq!(g.bounds(b), Some((x, x)));
    assert!(g.contains(b));
    assert_eq!(g.upper(a), [c]);
    assert_eq!(g.lower(c), [a]);
}

#[test]
fn rollback_undoes_and_redo_restores() {
    let (mut u, [a, b], x) = setup(["A", "B"]);
    u.gadt_add(&unbounded(&[a]), 0);
    let mark = u.gadt().mark();
    u.gadt_add(&unbounded(&[b]), 0);
    u.gadt_mut().order(a, b, Unification::None);
    u.gadt_update_entry(a, x, ANY);
    u.gadt_mut().set_narrowing();
    let changes = u.gadt().changes_since(mark).expect("changed");
    u.gadt_mut().rollback(mark);
    assert_eq!(u.gadt().symbols().collect::<Vec<_>>(), [a]);
    assert_eq!(u.gadt().bounds(a), Some((NOTHING, ANY)));
    assert!(u.gadt().upper(a).is_empty() && !u.gadt().is_narrowing());

    u.gadt_mut().redo(changes);
    assert_eq!(u.gadt().symbols().collect::<Vec<_>>(), [a, b]);
    assert_eq!(u.gadt().bounds(a), Some((x, ANY)));
    assert!(u.gadt().is_less(a, b) && u.gadt().is_narrowing());
    assert!(u.gadt().changes_since(u.gadt().mark()).is_none());
}
