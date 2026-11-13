mod support;

use std::path::Path;
use support::{parse, parse_in, universe};
use tasty::{ANY, Flags, Kind, NOTHING, Symbol, Type, TypeId, Universe};

fn stdlib() -> Option<Universe> {
    let jar = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs/scala-library-3.9.0.jar");
    if !jar.exists() {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", jar.display());
        return None;
    }
    Some(Universe::open(&[jar]).unwrap())
}

/// The match type a type member of `object pkg.obj` aliases, under its type lambda.
fn match_alias(u: &mut Universe, pkg: &str, obj: &str, name: &str) -> TypeId {
    let p = u.package(pkg).unwrap();
    let module = u.decl(p, obj).unwrap().into_iter().find(|&s| u.symbol(s).kind == Kind::Val).unwrap();
    let class = u.module_class(module).unwrap();
    let alias = u.decl(class, name).unwrap()[0];
    let mut t = u.info(alias);
    loop {
        match *u.types().get(t) {
            Type::Alias(a) | Type::Bounds(_, a) => t = a,
            Type::Lambda { result, .. } => t = result,
            Type::Match { .. } => return t,
            other => panic!("{name}: {other:?}"),
        }
    }
}

/// A reference to the type member `name` of `object pkg.obj`.
fn member_ref(u: &mut Universe, pkg: &str, obj: &str, name: &str) -> TypeId {
    let p = u.package(pkg).unwrap();
    let module = u.decl(p, obj).unwrap().into_iter().find(|&s| u.symbol(s).kind == Kind::Val).unwrap();
    let class = u.module_class(module).unwrap();
    let alias = u.decl(class, name).unwrap()[0];
    u.intern_type(Type::Sym(None, alias))
}

fn class(u: &mut Universe, full: &str) -> TypeId {
    u.class_ref(full)
}

fn applied(u: &mut Universe, tycon: TypeId, args: &[TypeId]) -> TypeId {
    u.intern_applied(tycon, args)
}

fn int(u: &mut Universe, n: i64) -> TypeId {
    u.intern_type(Type::Constant(tasty::tags::INTconst, n))
}

#[test]
fn tuple_match_types_reduce() {
    let Some(mut u) = stdlib() else { return };
    let u = &mut u;
    let (int_t, string, boolean) =
        (class(u, "scala.Int"), class(u, "java.lang.String"), class(u, "scala.Boolean"));
    let tuple2 = class(u, "scala.Tuple2");
    let pair = applied(u, tuple2, &[int_t, string]);
    let head = member_ref(u, "scala", "Tuple", "Head");
    let t = applied(u, head, &[pair]);
    let n = u.normalized(t);
    assert_eq!(n, int_t, "{}", u.show(n));
    assert!(u.is_subtype(int_t, t) && u.is_subtype(t, int_t));
    assert!(!u.is_subtype(string, t));

    let tail = member_ref(u, "scala", "Tuple", "Tail");
    let t = applied(u, tail, &[pair]);
    let tail_t = u.normalized(t);
    let shown = u.show(tail_t);
    assert!(shown.contains("String") && shown.contains("EmptyTuple"), "{shown}");

    let size = member_ref(u, "scala", "Tuple", "Size");
    let t = applied(u, size, &[pair]);
    let two = int(u, 2);
    let n = u.normalized(t);
    assert_eq!(n, two, "{}", u.show(n));

    let elem = member_ref(u, "scala", "Tuple", "Elem");
    let one = int(u, 1);
    let t = applied(u, elem, &[pair, one]);
    let n = u.normalized(t);
    assert_eq!(n, string, "{}", u.show(n));

    let triple = class(u, "scala.Tuple3");
    let triple = applied(u, triple, &[int_t, boolean, int_t]);
    let union = member_ref(u, "scala", "Tuple", "Union");
    let t = applied(u, union, &[triple]);
    let union_t = u.normalized(t);
    let or = u.intern_type(Type::Or(int_t, boolean, false));
    assert!(u.is_subtype(union_t, or) && u.is_subtype(or, union_t), "{}", u.show(union_t));
    assert!(u.is_subtype(int_t, t));
}

#[test]
fn stuck_and_disjoint_match_types() {
    let Some(mut u) = stdlib() else { return };
    let u = &mut u;
    let head = member_ref(u, "scala", "Tuple", "Head");
    let tuple = class(u, "scala.Tuple");
    let t = applied(u, head, &[tuple]);
    assert_eq!(u.try_normalize(t), None, "{}", u.show(t));
    let int_t = class(u, "scala.Int");
    let t = applied(u, head, &[int_t]);
    assert_eq!(u.try_normalize(t), None);
}

fn has_undecoded(u: &mut Universe, t: TypeId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= matches!(u.types().get(part), Type::Other(_));
        None
    });
    found
}

#[test]
fn library_match_types_keep_scrutinee_and_cases() {
    let Some(mut u) = stdlib() else { return };
    for (pkg, obj, name, n) in [
        ("scala", "Tuple", "Head", 1),
        ("scala", "Tuple", "Tail", 1),
        ("scala", "Tuple", "Size", 2),
        ("scala", "Tuple", "Elem", 1),
        ("scala", "Tuple", "Zip", 2),
        ("scala", "Tuple", "Fold", 2),
    ] {
        let t = match_alias(&mut u, pkg, obj, name);
        let Type::Match { cases, .. } = *u.types().get(t) else { unreachable!() };
        assert_eq!(u.types_of(cases).len(), n, "{name}: {}", u.show(t));
        assert!(!has_undecoded(&mut u, t), "{name}: {}", u.show(t));
    }
    let head = match_alias(&mut u, "scala", "Tuple", "Head");
    let Type::Match { cases, .. } = *u.types().get(head) else { unreachable!() };
    let Some(lambda) = u.lambda(u.types_of(cases)[0]) else { panic!("{}", u.show(head)) };
    assert_eq!(lambda.params.len(), 2);
    assert!(matches!(u.types().get(lambda.result), Type::MatchCase(..)));
}

/// A case of a match type: its captures, pattern and body.
type Case<'a> = (&'a [&'a str], &'a str, &'a str);

const OTHERWISE: Case = (&[], "scala.Any", "\"other\"");

/// The case `[captures] =>> pat => body`, its captures unbounded.
fn case(u: &mut Universe, (captures, pat, body): Case) -> TypeId {
    let binder = u.fresh_binder();
    let (pat, body) = (parse_in(u, pat, binder, captures), parse_in(u, body, binder, captures));
    let case = u.intern_type(Type::MatchCase(pat, body));
    if captures.is_empty() {
        return case;
    }
    let unbounded = u.intern_type(Type::Bounds(NOTHING, ANY));
    let params: Vec<_> = captures.iter().map(|c| (u.intern_name(c), unbounded)).collect();
    u.intern_lambda(binder, &params, case)
}

/// Checks that `scrutinee match { cases }` reduces to `expected`, or is stuck for `None`.
fn check_reduction(u: &mut Universe, scrutinee: &str, cases: &[Case], expected: Option<&str>) {
    let s = parse(u, scrutinee);
    let cases: Vec<TypeId> = cases.iter().map(|&c| case(u, c)).collect();
    let m = u.intern_match(ANY, s, &cases);
    let shown = u.show(m);
    let reduced = u.try_normalize(m);
    match (reduced, expected) {
        (Some(r), Some(e)) => {
            let e = parse(u, e);
            assert!(u.is_subtype(r, e) && u.is_subtype(e, r), "{shown}: {}", u.show(r));
        }
        (None, None) => {}
        (r, _) => panic!("{shown}: {:?} instead of {expected:?}", r.map(|r| u.show(r))),
    }
}

#[test]
fn cases_are_skipped_only_when_provably_disjoint() {
    let Some(mut u) = universe() else { return };
    let test = |p: &'static str, b| (&[] as &[&str], p, b);
    let dotty: [(&str, &[Case], Option<&str>); 17] = [
        ("scala.Int", &[test("java.lang.String", "1"), test("scala.Int", "2")], Some("2")),
        ("java.lang.String", &[test("scala.Int", "1"), test("java.lang.String", "2")], Some("2")),
        ("oracle.Dog", &[test("oracle.Animal", "1")], Some("1")),
        ("oracle.Animal", &[test("oracle.Dog", "1"), OTHERWISE], None),
        ("oracle.Box", &[test("oracle.Outer", "1"), OTHERWISE], Some("\"other\"")),
        ("oracle.Box", &[test("oracle.Animal", "1"), OTHERWISE], None),
        ("oracle.O.type", &[test("oracle.Animal", "1"), OTHERWISE], Some("\"other\"")),
        ("oracle.O.type", &[test("oracle.Box", "1"), OTHERWISE], Some("1")),
        ("1", &[test("2", "\"a\""), test("1", "\"b\"")], Some("\"b\"")),
        ("1", &[test("scala.Int", "\"a\"")], Some("\"a\"")),
        ("scala.Option[scala.Int]", &[test("scala.Some[scala.Int]", "1"), OTHERWISE], None),
        ("scala.Some[scala.Int]", &[test("scala.None.type", "1"), OTHERWISE], Some("\"other\"")),
        ("scala.Int | java.lang.String", &[test("scala.Boolean", "1"), OTHERWISE], Some("\"other\"")),
        ("scala.Int | java.lang.String", &[test("scala.Int", "1"), OTHERWISE], None),
        ("oracle.Inv[scala.Int]", &[test("oracle.Inv[java.lang.String]", "1"), OTHERWISE], Some("\"other\"")),
        ("oracle.Cov[scala.Int]", &[test("oracle.Cov[java.lang.String]", "1"), OTHERWISE], None),
        ("scala.Some[scala.Int]", &[test("scala.Some[java.lang.String]", "1"), OTHERWISE], Some("\"other\"")),
    ];
    for (scrutinee, cases, expected) in dotty {
        check_reduction(&mut u, scrutinee, cases, expected);
    }
}

#[test]
fn captures_take_the_scrutinee_arguments() {
    let Some(mut u) = universe() else { return };
    let s = "scala.compiletime.ops.int.S[n]";
    let dotty: [(&str, &[Case], Option<&str>); 10] = [
        ("scala.Option[scala.Int]", &[(&["X"], "scala.Option[X]", "X")], Some("scala.Int")),
        ("scala.Some[java.lang.String]", &[(&["X"], "scala.Option[X]", "X")], Some("java.lang.String")),
        (
            "scala.collection.immutable.List[scala.Int]",
            &[(&["X"], "scala.collection.immutable.Seq[X]", "X")],
            Some("scala.Int"),
        ),
        ("oracle.Inv[scala.Int]", &[(&["X"], "oracle.Inv[X]", "X")], Some("scala.Int")),
        (
            "scala.Tuple2[scala.Int, java.lang.String]",
            &[(&["A", "B"], "scala.Tuple2[A, B]", "B")],
            Some("java.lang.String"),
        ),
        (
            "scala.Option[scala.Int]",
            &[(&["X"], "scala.collection.immutable.List[X]", "X"), OTHERWISE],
            Some("\"other\""),
        ),
        ("oracle.Animal", &[(&["X"], "oracle.Inv[X]", "X"), OTHERWISE], None),
        ("3", &[(&["n"], s, "n")], Some("2")),
        ("0", &[(&["n"], s, "n"), OTHERWISE], None),
        ("0", &[(&[], "0", "\"zero\""), (&["n"], s, "n")], Some("\"zero\"")),
    ];
    for (scrutinee, cases, expected) in dotty {
        check_reduction(&mut u, scrutinee, cases, expected);
    }
}

#[test]
fn tail_recursive_match_types_reduce_deeper_than_nested_reductions() {
    let Some(mut u) = universe() else { return };
    let root = u.root();
    let down = u.define(Symbol {
        name: "Down".into(),
        kind: Kind::Type,
        owner: Some(root),
        flags: Flags::default(),
        target: None,
        is_extern: false,
        derived: false,
        decls: Vec::new(),
        tree: None,
    });
    let down_ref = u.intern_type(Type::Sym(None, down));
    let binder = u.fresh_binder();
    let n = u.intern_type(Type::Param(binder, 0));
    let zero = case(&mut u, (&[], "0", "\"done\""));
    let step_binder = u.fresh_binder();
    let pred = u.intern_type(Type::Param(step_binder, 0));
    let s = parse_in(&mut u, "scala.compiletime.ops.int.S[n]", step_binder, &["n"]);
    let recursion = u.intern_applied(down_ref, &[pred]);
    let step = u.intern_type(Type::MatchCase(s, recursion));
    let unbounded = u.intern_type(Type::Bounds(NOTHING, ANY));
    let n_name = u.intern_name("n");
    let step = u.intern_lambda(step_binder, &[(n_name, unbounded)], step);
    let body = u.intern_match(ANY, n, &[zero, step]);
    let info = u.intern_lambda(binder, &[(n_name, unbounded)], body);
    u.set_info(down, info);
    let hundred = int(&mut u, 100);
    let t = u.intern_applied(down_ref, &[hundred]);
    let done = u.str_type("done");
    assert_eq!(u.try_normalize(t), Some(done), "{}", u.show(t));
}
