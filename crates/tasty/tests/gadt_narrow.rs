//! The comparer narrowing the GADT constraint in GADT mode (dotty's `narrowGADTBounds` through
//! `GadtState`): bounds, orderings, unification, `necessaryEither`, `ApproxState` and
//! applications of higher-kinded symbols.

mod support;

use support::{define, named_type, parse, parse_with, universe};
use tasty::{ANY, Kind, Mark, NOTHING, NoVars, Registered, SymbolId, Type, TypeId, Universe};

/// A symbol of a row: a type parameter registered at `level`, else an abstract type or a value
/// of type `hi`, with its declared bounds (empty for `Nothing` and `Any`).
#[derive(Clone, Copy)]
struct Sym {
    name: &'static str,
    kind: Kind,
    level: Option<u32>,
    lo: &'static str,
    hi: &'static str,
}

const fn param(name: &'static str, level: u32) -> Sym {
    Sym { name, kind: Kind::TypeParam, level: Some(level), lo: "", hi: "" }
}

const fn below(name: &'static str, hi: &'static str) -> Sym {
    Sym { name, kind: Kind::Type, level: None, lo: "", hi }
}

const fn above(name: &'static str, lo: &'static str) -> Sym {
    Sym { name, kind: Kind::Type, level: None, lo, hi: "" }
}

const fn value(name: &'static str, tpe: &'static str) -> Sym {
    Sym { name, kind: Kind::Val, level: None, lo: "", hi: tpe }
}

/// Comparisons in GADT mode and what the GADT constraint holds after them.
struct Row {
    name: &'static str,
    syms: &'static [Sym],
    /// `a <: b` with its expected result, in order.
    steps: &'static [(&'static str, &'static str, bool)],
    /// The bounds without the ordered symbols, an instance's twice.
    bounds: &'static [(&'static str, &'static str, &'static str)],
    /// Every pair of registered symbols ordered one below the other.
    less: &'static [(&'static str, &'static str)],
    /// The symbols made equal to a type.
    instances: &'static [(&'static str, &'static str)],
}

const NONE: Row = Row { name: "", syms: &[], steps: &[], bounds: &[], less: &[], instances: &[] };

/// A universe's symbols defined for one row, with references to them for the parser.
struct Scope {
    syms: Vec<(&'static str, SymbolId)>,
    locals: Vec<(&'static str, TypeId)>,
}

impl Scope {
    /// Defines `syms`, sets their declared bounds and registers the type parameters.
    fn new(u: &mut Universe, syms: &[Sym]) -> Scope {
        let root = u.root();
        let unbounded = u.intern_type(Type::Bounds(NOTHING, ANY));
        let mut scope = Scope { syms: Vec::new(), locals: Vec::new() };
        for s in syms {
            let id = define(u, s.name, s.kind, root, unbounded);
            let r = u.intern_type(Type::Sym(None, id));
            scope.syms.push((s.name, id));
            scope.locals.push((s.name, r));
        }
        for (s, &(_, id)) in syms.iter().zip(&scope.syms.clone()) {
            let (lo, hi) = (scope.ty(u, s.lo, NOTHING), scope.ty(u, s.hi, ANY));
            let info = if s.kind == Kind::Val { hi } else { u.intern_type(Type::Bounds(lo, hi)) };
            u.set_info(id, info);
            if let Some(level) = s.level {
                u.gadt_add(&[Registered { sym: id, lo, hi }], level);
            }
        }
        scope
    }

    fn ty(&self, u: &mut Universe, s: &str, empty: TypeId) -> TypeId {
        match s {
            "" => empty,
            "Nothing" => NOTHING,
            "Any" => ANY,
            _ => parse_with(u, s, &self.locals),
        }
    }

    fn sym(&self, name: &str) -> SymbolId {
        self.syms.iter().find(|&&(n, _)| n == name).unwrap().1
    }

    fn holds(&self, u: &mut Universe, a: &str, b: &str) -> bool {
        let (a, b) = (self.ty(u, a, NOTHING), self.ty(u, b, ANY));
        u.constrain_gadt_in(a, b, &mut NoVars)
    }
}

fn check(u: &mut Universe, row: &Row) {
    u.gadt_mut().rollback(Mark(0));
    let scope = Scope::new(u, row.syms);
    let name = row.name;
    for &(a, b, expected) in row.steps {
        assert_eq!(scope.holds(u, a, b), expected, "{name}: {a} <: {b}");
    }
    for &(s, lo, hi) in row.bounds {
        let expected = (scope.ty(u, lo, NOTHING), scope.ty(u, hi, ANY));
        let actual = u.gadt().bounds(scope.sym(s)).unwrap();
        let show = |u: &mut Universe, (l, h)| format!("{} .. {}", u.show(l), u.show(h));
        assert_eq!(actual, expected, "{name}: bounds of {s}: {}", show(u, actual));
    }
    let registered: Vec<_> = row.syms.iter().filter(|s| s.level.is_some()).map(|s| s.name).collect();
    for &a in &registered {
        for &b in &registered {
            let less = u.gadt().is_less(scope.sym(a), scope.sym(b));
            assert_eq!(less, row.less.contains(&(a, b)), "{name}: {a} < {b}");
        }
        let instance = u.gadt().entry(scope.sym(a)).unwrap().instance;
        let expected = row.instances.iter().find(|&&(s, _)| s == a).map(|&(_, i)| scope.ty(u, i, ANY));
        assert_eq!(instance, expected, "{name}: instance of {a}");
    }
}

const T: Sym = param("T", 0);
const A: Sym = param("A", 0);
const B: Sym = param("B", 0);
const C: Sym = param("C", 0);

/// Rows worked out from dotty's `GadtState.addBound`, `ConstraintHandling.addLess`, `unify`,
/// `addBoundTransitively`, `addOneBound` and `TypeComparer.necessaryEither`.
const ROWS: &[Row] = &[
    Row {
        name: "upper",
        syms: &[T],
        steps: &[("T", "scala.Int", true)],
        bounds: &[("T", "Nothing", "scala.Int")],
        ..NONE
    },
    Row {
        name: "lower",
        syms: &[T],
        steps: &[("scala.Int", "T", true)],
        bounds: &[("T", "scala.Int", "Any")],
        ..NONE
    },
    Row {
        name: "both",
        syms: &[T],
        steps: &[("T", "oracle.Animal", true), ("oracle.Dog", "T", true)],
        bounds: &[("T", "oracle.Dog", "oracle.Animal")],
        ..NONE
    },
    Row {
        name: "upper bounds met",
        syms: &[T],
        steps: &[("T", "oracle.Animal", true), ("T", "oracle.Dog", true)],
        bounds: &[("T", "Nothing", "oracle.Dog")],
        ..NONE
    },
    Row {
        name: "empty bounds refused",
        syms: &[T],
        steps: &[("T", "oracle.Dog", true), ("oracle.Animal", "T", false)],
        bounds: &[("T", "Nothing", "oracle.Dog")],
        ..NONE
    },
    Row {
        name: "equal bounds replace",
        syms: &[T],
        steps: &[("T", "oracle.Dog", true), ("oracle.Dog", "T", true)],
        bounds: &[("T", "oracle.Dog", "oracle.Dog")],
        instances: &[("T", "oracle.Dog")],
        ..NONE
    },
    Row {
        name: "addLess",
        syms: &[A, B],
        steps: &[("A", "B", true)],
        bounds: &[("A", "Nothing", "Any"), ("B", "Nothing", "Any")],
        less: &[("A", "B")],
        ..NONE
    },
    Row {
        name: "addLess from the right's lower bound",
        syms: &[A, B],
        steps: &[("scala.Int", "B", true), ("B", "A", true)],
        bounds: &[("A", "scala.Int", "Any"), ("B", "scala.Int", "Any")],
        less: &[("B", "A")],
        ..NONE
    },
    Row {
        name: "addLess propagates both bounds",
        syms: &[A, B],
        steps: &[("B", "oracle.Animal", true), ("oracle.Dog", "A", true), ("A", "B", true)],
        bounds: &[("A", "oracle.Dog", "oracle.Animal"), ("B", "oracle.Dog", "oracle.Animal")],
        less: &[("A", "B")],
        ..NONE
    },
    Row {
        name: "addLess refused when the bounds empty",
        syms: &[A, B],
        steps: &[("B", "oracle.Dog", true), ("oracle.Animal", "A", true), ("A", "B", false)],
        bounds: &[("A", "oracle.Animal", "Any"), ("B", "Nothing", "oracle.Dog")],
        ..NONE
    },
    Row {
        name: "addLess transitive",
        syms: &[A, B, C],
        steps: &[("C", "scala.Int", true), ("A", "B", true), ("B", "C", true)],
        bounds: &[
            ("A", "Nothing", "scala.Int"),
            ("B", "Nothing", "scala.Int"),
            ("C", "Nothing", "scala.Int"),
        ],
        less: &[("A", "B"), ("B", "C"), ("A", "C")],
        ..NONE
    },
    Row {
        name: "lower bound added transitively to the symbols above",
        syms: &[A, B],
        steps: &[("A", "B", true), ("scala.Int", "A", true)],
        bounds: &[("A", "scala.Int", "Any"), ("B", "scala.Int", "Any")],
        less: &[("A", "B")],
        ..NONE
    },
    Row {
        name: "upper bound added transitively to the symbols below",
        syms: &[A, B],
        steps: &[("A", "B", true), ("B", "scala.Int", true)],
        bounds: &[("A", "Nothing", "scala.Int"), ("B", "Nothing", "scala.Int")],
        less: &[("A", "B")],
        ..NONE
    },
    Row {
        name: "unify keeps the first of one level with both bounds",
        syms: &[A, B],
        steps: &[("A", "oracle.Animal", true), ("oracle.Dog", "B", true), ("A", "B", true), ("B", "A", true)],
        bounds: &[("A", "oracle.Dog", "oracle.Animal")],
        instances: &[("B", "A")],
        ..NONE
    },
    Row {
        name: "unify keeps the shallower",
        syms: &[param("A", 1), param("B", 0)],
        steps: &[("A", "oracle.Animal", true), ("oracle.Dog", "B", true), ("A", "B", true), ("B", "A", true)],
        bounds: &[("B", "oracle.Dog", "oracle.Animal")],
        instances: &[("A", "B")],
        ..NONE
    },
    Row {
        name: "unify keeps the orderings of the kept symbol",
        syms: &[A, B, C],
        steps: &[("C", "A", true), ("A", "B", true), ("B", "A", true)],
        bounds: &[("A", "Nothing", "Any"), ("C", "Nothing", "Any")],
        less: &[("C", "A")],
        instances: &[("B", "A")],
    },
    Row {
        name: "either: the left part of an intersection",
        syms: &[T],
        steps: &[("T & oracle.Dog", "scala.Int", true)],
        bounds: &[("T", "Nothing", "scala.Int")],
        ..NONE
    },
    Row {
        name: "either: the right part of an intersection",
        syms: &[T],
        steps: &[("oracle.Dog & T", "scala.Int", true)],
        bounds: &[("T", "Nothing", "scala.Int")],
        ..NONE
    },
    Row {
        name: "either: the weaker second branch",
        syms: &[T],
        steps: &[("T", "scala.Int | scala.Matchable", true)],
        bounds: &[("T", "Nothing", "scala.Matchable")],
        ..NONE
    },
    Row {
        name: "either: the weaker first branch",
        syms: &[T],
        steps: &[("T", "scala.Matchable | scala.Int", true)],
        bounds: &[("T", "Nothing", "scala.Matchable")],
        ..NONE
    },
    Row {
        name: "either: neither branch subsumes",
        syms: &[T],
        steps: &[("T", "scala.Int | java.lang.String", true)],
        bounds: &[("T", "Nothing", "Any")],
        ..NONE
    },
    Row {
        name: "approx: through an abstract type's upper bound",
        syms: &[T, below("U", "T")],
        steps: &[("U", "scala.Int", false)],
        bounds: &[("T", "Nothing", "Any")],
        ..NONE
    },
    Row {
        name: "approx: through an abstract type's lower bound",
        syms: &[T, above("L", "T")],
        steps: &[("scala.Int", "L", false)],
        bounds: &[("T", "Nothing", "Any")],
        ..NONE
    },
    Row {
        name: "approx: fresh for the type arguments of a bound",
        syms: &[T, above("L", "oracle.Inv[T]")],
        steps: &[("oracle.Inv[scala.Int]", "L", true)],
        bounds: &[("T", "scala.Int", "scala.Int")],
        instances: &[("T", "scala.Int")],
        ..NONE
    },
    Row {
        name: "approx: low through a singleton's underlying type",
        syms: &[T, value("x", "T")],
        steps: &[("x", "scala.Int", false)],
        bounds: &[("T", "Nothing", "Any")],
        ..NONE
    },
];

#[test]
fn gadt_mode_comparisons_narrow_as_dotty() {
    let Some(mut u) = universe() else { return };
    for row in ROWS {
        check(&mut u, row);
    }
}

/// `G#T` and `oracle.Box#T`, for type parameter `G`.
fn box_members(u: &mut Universe, g: TypeId) -> [TypeId; 2] {
    let boxed = named_type(u, "oracle.Box");
    let (_, box_class) = u.sym_ref(boxed).unwrap();
    let t = u.decl(box_class, "T").unwrap()[0];
    [g, boxed].map(|p| u.intern_type(Type::Sym(Some(p), t)))
}

#[test]
fn approx_state_is_fresh_at_prefixes_and_refinement_members() {
    let Some(mut u) = universe() else { return };
    let scope = Scope::new(&mut u, &[param("G", 0)]);
    let (g, g_ref) = (scope.sym("G"), scope.locals[0].1);
    let [g_t, box_t] = box_members(&mut u, g_ref);
    let root = u.root();
    let u_sym = define(&mut u, "U", Kind::Type, root, ANY);
    let u_bounds = u.intern_type(Type::Bounds(NOTHING, g_t));
    u.set_info(u_sym, u_bounds);
    let u_ref = u.intern_type(Type::Sym(None, u_sym));
    assert!(u.constrain_gadt_in(u_ref, box_t, &mut NoVars), "U <: G#T, U <: Box#T: G <: Box");
    let boxed = named_type(&mut u, "oracle.Box");
    assert_eq!(u.gadt().bounds(g), Some((NOTHING, boxed)));

    u.gadt_mut().rollback(Mark(0));
    let scope = Scope::new(&mut u, &[param("G", 0)]);
    let (g, g_ref) = (scope.sym("G"), scope.locals[0].1);
    let name = u.intern_name("T");
    let refined = |u: &mut Universe, x: TypeId| {
        let alias = u.intern_type(Type::Alias(x));
        u.intern_type(Type::Refined(boxed, name, alias))
    };
    let int = parse(&mut u, "scala.Int");
    let (box_g, box_int) = (refined(&mut u, g_ref), refined(&mut u, int));
    let v = define(&mut u, "V", Kind::Type, root, ANY);
    let v_bounds = u.intern_type(Type::Bounds(NOTHING, box_g));
    u.set_info(v, v_bounds);
    let v_ref = u.intern_type(Type::Sym(None, v));
    assert!(u.constrain_gadt_in(v_ref, box_int, &mut NoVars), "V <: Box {{ type T = G }}");
    assert_eq!(u.gadt().entry(g).and_then(|e| e.instance), Some(int));
}

#[test]
fn applications_of_gadt_symbols_compare_their_arguments_frozen_unless_injective() {
    let Some(mut u) = universe() else { return };
    let binder = u.fresh_binder();
    let x = u.intern_name("X");
    let any_lambda = u.intern_lambda(binder, &[(x, ANY)], ANY);
    let option = parse(&mut u, "scala.Option");
    let root = u.root();
    let hk = u.intern_type(Type::Bounds(NOTHING, any_lambda));
    // (F's GADT bounds, G's, comparison, holds, A's GADT bounds after) from dotty's
    // `isMatchingApply` (`tyconIsInjective`), `compareAppliedType1/2` and `byGadtBounds`.
    type Bounds = Option<(TypeId, TypeId)>;
    type Expected = (&'static str, &'static str);
    let rows: &[(Bounds, Bounds, &str, bool, Expected)] = &[
        (Some((option, option)), None, "F[A] <: scala.Option[scala.Int]", true, ("", "scala.Int")),
        (Some((NOTHING, option)), None, "F[A] <: scala.Option[scala.Int]", false, ("", "")),
        (None, Some((option, option)), "scala.Option[scala.Int] <: G[A]", true, ("scala.Int", "scala.Int")),
        (None, Some((option, any_lambda)), "scala.Option[scala.Int] <: G[A]", false, ("", "")),
        (Some((NOTHING, option)), None, "F[A] <: scala.Option[A]", true, ("", "")),
        (None, Some((option, any_lambda)), "scala.Option[A] <: G[A]", true, ("", "")),
    ];
    for &(f_bounds, g_bounds, cmp, holds, (a_lo, a_hi)) in rows {
        u.gadt_mut().rollback(Mark(0));
        let [f, g] = ["F", "G"].map(|n| define(&mut u, n, Kind::TypeParam, root, hk));
        let mut registered = Vec::new();
        for (s, bounds) in [(f, f_bounds), (g, g_bounds)] {
            let (lo, hi) = bounds.unwrap_or((NOTHING, any_lambda));
            registered.push(Registered { sym: s, lo, hi });
        }
        u.gadt_add(&registered, 0);
        let scope = Scope::new(&mut u, &[param("A", 0)]);
        let locals: Vec<(&str, TypeId)> = [("F", f), ("G", g), ("A", scope.sym("A"))]
            .map(|(n, s)| (n, u.intern_type(Type::Sym(None, s))))
            .to_vec();
        let (a, b) = cmp.split_once(" <: ").unwrap();
        let (a, b) = (parse_with(&mut u, a, &locals), parse_with(&mut u, b, &locals));
        assert_eq!(u.constrain_gadt_in(a, b, &mut NoVars), holds, "{cmp}");
        let expected = (scope.ty(&mut u, a_lo, NOTHING), scope.ty(&mut u, a_hi, ANY));
        assert_eq!(u.gadt().bounds(scope.sym("A")), Some(expected), "{cmp}: A");
    }
}
