use std::path::Path;
use tasty::{Kind, Type, TypeId, Universe};

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
        match u.types().get(t).clone() {
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
    u.intern_type(Type::Applied(tycon, args.to_vec()))
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
        let Type::Match { cases, .. } = u.types().get(t).clone() else { unreachable!() };
        assert_eq!(cases.len(), n, "{name}: {}", u.show(t));
        assert!(!has_undecoded(&mut u, t), "{name}: {}", u.show(t));
    }
    let head = match_alias(&mut u, "scala", "Tuple", "Head");
    let Type::Match { cases, .. } = u.types().get(head).clone() else { unreachable!() };
    let Type::Lambda { params, result, .. } = u.types().get(cases[0]).clone() else {
        panic!("{}", u.show(head))
    };
    assert_eq!(params.len(), 2);
    assert!(matches!(u.types().get(result), Type::MatchCase(..)));
}
