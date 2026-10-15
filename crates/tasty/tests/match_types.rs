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
