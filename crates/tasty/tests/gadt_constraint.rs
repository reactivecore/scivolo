//! The GADT constraint's ordering of symbols (dotty's `OrderingConstraint` under a
//! `GadtConstraint`).

mod support;

use support::define;
use tasty::tags::CASE;
use tasty::{ANY, Flags, Kind, NOTHING, Registered, SymbolId, Type, TypeId, Unification, Universe};

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
fn replacing_a_symbol_inside_a_declared_union_is_inexact() {
    let (mut u, [a, b, c], x) = setup(["A", "B", "C"]);
    let a_ref = type_ref(&mut u, a);
    let a_or_x = u.intern_type(Type::Or(a_ref, x, false));
    u.gadt_add(
        &[Registered { sym: a, lo: NOTHING, hi: ANY }, Registered { sym: b, lo: NOTHING, hi: a_or_x }],
        0,
    );
    u.gadt_add(&unbounded(&[c]), 0);
    let mark = u.gadt().mark();
    u.gadt_replace(c, x);
    assert!(!u.gadt().is_inexact());
    u.gadt_replace(a, x);
    assert!(u.gadt().is_inexact());
    u.gadt_mut().rollback(mark);
    assert!(!u.gadt().is_inexact());
}

#[test]
fn full_bounds_take_the_nearest_ordered_symbols_latest_first() {
    let (mut u, [a, b, c, d, e, f], x) = setup(["A", "B", "C", "D", "E", "F"]);
    let mut syms = unbounded(&[a, b, d, e, f]);
    syms.push(Registered { sym: c, lo: NOTHING, hi: x });
    u.gadt_add(&syms, 0);
    for (lo, hi) in [(a, c), (b, c), (c, d), (c, e), (f, a)] {
        u.gadt_mut().order(lo, hi, Unification::None);
    }
    assert_eq!(u.gadt().lower(c), [f, b, a]);
    assert_eq!(u.gadt().min_lower(c), [b, a]);
    assert_eq!(u.gadt().upper(c), [e, d]);

    let [a_ref, b_ref, d_ref, e_ref] = [a, b, d, e].map(|s| type_ref(&mut u, s));
    let lo = u.intern_type(Type::Or(b_ref, a_ref, true));
    let x_and_e = u.intern_type(Type::And(x, e_ref));
    let hi = u.intern_type(Type::And(x_and_e, d_ref));
    let (actual_lo, actual_hi) = u.gadt_full_bounds(c).unwrap();
    assert_eq!((actual_lo, actual_hi), (lo, hi), "{} .. {}", u.show(actual_lo), u.show(actual_hi));
    let [c_ref, f_ref] = [c, f].map(|s| type_ref(&mut u, s));
    assert_eq!(u.gadt_full_bounds(d), Some((c_ref, ANY)));
    assert_eq!(u.gadt_full_bounds(a), Some((f_ref, c_ref)));
}

#[test]
fn full_bounds_leave_out_a_pattern_bound_type_ordered_by_its_own_bound() {
    let (mut u, [b1], _) = setup(["B1"]);
    let b1_ref = type_ref(&mut u, b1);
    let root = u.root();
    let [b2, b3] = [(NOTHING, b1_ref), (b1_ref, ANY)].map(|(lo, hi)| {
        let info = u.intern_type(Type::Bounds(lo, hi));
        define(&mut u, "B", Kind::Type, root, info)
    });
    let syms = [
        Registered { sym: b1, lo: NOTHING, hi: ANY },
        Registered { sym: b2, lo: NOTHING, hi: b1_ref },
        Registered { sym: b3, lo: b1_ref, hi: ANY },
    ];
    u.gadt_add(&syms, 0);
    assert!(u.gadt().is_less(b2, b1) && u.gadt().is_less(b1, b3));
    let [b2_ref, b3_ref] = [b2, b3].map(|s| type_ref(&mut u, s));
    assert_eq!(u.gadt_full_bounds(b1), Some((b2_ref, b3_ref)), "abstract types stay");

    for s in [b2, b3] {
        u.set_flags(s, Flags::default().with(CASE));
    }
    assert_eq!(u.gadt_full_bounds(b1), Some((NOTHING, ANY)), "pattern-bound types left out");
    assert_eq!(u.gadt_full_bounds(b2), Some((NOTHING, b1_ref)));
}

#[test]
fn replace_substitutes_only_entries_that_are_the_symbol() {
    let (mut u, [a, b, c, d], x) = setup(["A", "B", "C", "D"]);
    let a_ref = type_ref(&mut u, a);
    let a_and_x = u.intern_type(Type::And(a_ref, x));
    let mut syms = unbounded(&[a, b, d]);
    syms.push(Registered { sym: c, lo: NOTHING, hi: a_and_x });
    u.gadt_add(&syms, 0);
    u.gadt_update_entry(b, a_ref, ANY);
    u.gadt_replace(d, a_ref);
    assert_eq!(u.gadt().bounds(d), Some((a_ref, a_ref)));

    u.gadt_replace(a, x);
    let g = u.gadt();
    assert_eq!(g.bounds(b), Some((x, ANY)), "a bound that is A");
    assert!(g.lower(b).is_empty());
    assert_eq!(g.bounds(c), Some((NOTHING, a_and_x)), "A inside a bound stays");
    assert_eq!(g.bounds(d), Some((x, x)), "an instance that is A");
    assert!(!g.is_inexact());
}

#[test]
fn rollback_undoes_a_replace_and_redo_makes_it_again() {
    let (mut u, [a, b, c], x) = setup(["A", "B", "C"]);
    u.gadt_add(&unbounded(&[a, b, c]), 0);
    u.gadt_mut().order(a, b, Unification::None);
    u.gadt_mut().order(b, c, Unification::None);
    let before = u.gadt().entry(b).cloned();
    let mark = u.gadt().mark();
    u.gadt_replace(b, x);
    let changes = u.gadt().changes_since(mark).expect("changed");

    u.gadt_mut().rollback(mark);
    assert_eq!(u.gadt().entry(b).cloned(), before);
    assert_eq!(u.gadt().upper(a), [c, b]);
    assert_eq!(u.gadt().lower(c), [b, a]);

    u.gadt_mut().redo(changes);
    assert_eq!(u.gadt().bounds(b), Some((x, x)));
    assert_eq!(u.gadt().upper(a), [c]);
}

#[test]
fn replacing_a_symbol_inside_a_declared_union_or_intersection_is_inexact() {
    /// Where `B`'s declared bound refers to `A`.
    #[derive(Clone, Copy, Debug)]
    enum Shape {
        UpperOr,
        UpperOrNested,
        UpperAnd,
        LowerAnd,
        LowerOr,
    }
    use Shape::*;
    // (shape, replaced, by `X` or else by the other symbol, inexact) after dotty's
    // `substDependentSyms`, which internalizes unions of upper and intersections of lower bounds.
    let rows = [
        (UpperOr, "A", true, true),
        (UpperOrNested, "A", true, true),
        (UpperAnd, "A", true, false),
        (LowerAnd, "A", true, true),
        (LowerOr, "A", true, false),
        (UpperOr, "C", false, true),
        (UpperOr, "A", false, true),
        (UpperOr, "C", true, false),
        (LowerAnd, "C", true, false),
    ];
    for (shape, replaced, by_x, inexact) in rows {
        let (mut u, [a, b, c], x) = setup(["A", "B", "C"]);
        let a_ref = type_ref(&mut u, a);
        let or = u.intern_type(Type::Or(a_ref, x, false));
        let and = u.intern_type(Type::And(x, a_ref));
        let (lo, hi) = match shape {
            UpperOr => (NOTHING, or),
            UpperOrNested => (NOTHING, u.intern_type(Type::Or(x, or, false))),
            UpperAnd => (NOTHING, and),
            LowerAnd => (and, ANY),
            LowerOr => (or, ANY),
        };
        u.gadt_add(&[Registered { sym: a, lo: NOTHING, hi: ANY }, Registered { sym: b, lo, hi }], 0);
        u.gadt_add(&unbounded(&[c]), 0);
        let (s, other) = if replaced == "A" { (a, c) } else { (c, a) };
        let by = if by_x { x } else { type_ref(&mut u, other) };
        u.gadt_replace(s, by);
        assert_eq!(u.gadt().is_inexact(), inexact, "{shape:?}: {replaced} replaced, by X: {by_x}");
    }

    let (mut u, [a, b], x) = setup(["A", "B"]);
    let a_ref = type_ref(&mut u, a);
    let or = u.intern_type(Type::Or(a_ref, x, false));
    u.gadt_add(&[Registered { sym: b, lo: NOTHING, hi: or }], 0);
    u.gadt_add(&unbounded(&[a]), 0);
    u.gadt_replace(a, x);
    assert!(!u.gadt().is_inexact(), "A registered after B");
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
