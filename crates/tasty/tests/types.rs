use std::path::Path;
use tasty::{Erased, Kind, SymbolId, Type, Universe};

fn stdlib() -> Option<Universe> {
    let jar = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs/scala-library-3.9.0.jar");
    if !jar.exists() {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", jar.display());
        return None;
    }
    Some(Universe::open(&[jar]).unwrap())
}

/// The members named `name` of the class `class` in package `pkg`.
fn members(u: &mut Universe, pkg: &str, class: &str, name: &str) -> Vec<SymbolId> {
    let p = u.package(pkg).unwrap();
    let c = u.decl(p, class).unwrap().into_iter().find(|&c| u.symbol(c).kind == Kind::Class).unwrap();
    u.decl(c, name).unwrap()
}

fn infos(u: &mut Universe, pkg: &str, class: &str, name: &str) -> Vec<String> {
    members(u, pkg, class, name)
        .into_iter()
        .map(|m| {
            let t = u.info(m);
            u.show(t)
        })
        .collect()
}

#[test]
fn declared_types_of_methods_values_and_types() {
    let Some(mut u) = stdlib() else { return };
    assert_eq!(
        infos(&mut u, "scala", "Option", "map"),
        ["[B: >: scala.Nothing <: scala.Any](f: scala.Function1[scala.Option.this.A, B]): scala.Option[B]"]
    );
    assert_eq!(
        infos(&mut u, "scala", "Option", "getOrElse"),
        ["[B: >: scala.Option.this.A <: scala.Any](default: => B): B"]
    );
    assert_eq!(
        infos(&mut u, "scala", "Predef$", "println"),
        ["(): scala.Unit", "(x: scala.Any): scala.Unit"]
    );
    assert_eq!(
        infos(&mut u, "scala", "Predef$", "implicitly"),
        ["[T: >: scala.Nothing <: scala.Any](implicit e: T): T"]
    );
    assert_eq!(infos(&mut u, "scala", "Predef$", "String"), ["java.lang.String"]);
    assert_eq!(infos(&mut u, "scala.collection.immutable", "List", "A"), [">: scala.Nothing <: scala.Any"]);
}

#[test]
fn annotated_types_keep_the_annotation_class() {
    let Some(mut u) = stdlib() else { return };
    let [m] = members(&mut u, "scala.collection", "IterableOps", "fromSpecific")[..] else {
        panic!("one fromSpecific")
    };
    let info = u.info(m);
    let shown = u.show(info);
    let Type::Method { params, .. } = u.types().get(info).clone() else { panic!("{shown}") };
    let (mut todo, mut annotations) = (vec![params[0].1], Vec::new());
    while let Some(t) = todo.pop() {
        match u.types().get(t).clone() {
            Type::Annotated(inner, a) => {
                todo.push(inner);
                annotations.push(a);
            }
            Type::Applied(tycon, args) => {
                todo.push(tycon);
                todo.extend(args);
            }
            _ => {}
        }
    }
    let classes: Vec<_> = annotations.into_iter().filter_map(|a| u.class_symbol(a)).collect();
    let classes: Vec<String> = classes.into_iter().map(|c| u.full_name(c)).collect();
    let unchecked = "scala.annotation.unchecked.uncheckedVariance".to_string();
    assert!(classes.contains(&unchecked), "{shown}: {classes:?}");
}

#[test]
fn erasure_of_aliases_arrays_and_type_parameters() {
    let Some(mut u) = stdlib() else { return };
    let erased_result = |u: &mut Universe, pkg, class, name| {
        let m = members(u, pkg, class, name)[0];
        let mut t = u.info(m);
        while let tasty::Type::Method { result, .. } | tasty::Type::Poly { result, .. } =
            u.types().get(t).clone()
        {
            t = result;
        }
        u.erase(t)
    };
    assert_eq!(
        erased_result(&mut u, "scala", "Function0", "toString"),
        Erased::Class("java.lang.String".into())
    );
    assert_eq!(
        erased_result(&mut u, "scala", "Option", "getOrElse"),
        Erased::Class("java.lang.Object".into())
    );
    assert_eq!(erased_result(&mut u, "scala", "Array", "clone"), Erased::Class("java.lang.Object".into()));
    let from = members(&mut u, "scala", "Array$", "from")[0];
    assert_eq!(
        u.method_signature(from).as_deref(),
        Some("D4fromL29scala.collection.IterableOnceL22scala.reflect.ClassTagL16java.lang.ObjectEO")
    );
}
