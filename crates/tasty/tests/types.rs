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
fn members_resolve_through_an_applied_alias_of_a_refinement() {
    let Some(mut u) = stdlib() else { return };
    let member = members(&mut u, "scala.deriving", "Mirror", "MirroredLabel")[0];
    let alias = members(&mut u, "scala.deriving", "Mirror$", "Of")[0];
    let alias = u.intern_type(Type::Sym(None, alias));
    let int = u.class_ref("scala.Int");
    let prefix = u.intern_applied(alias, &[int]);
    let name = u.interned_name(member);
    let selected = u.intern_type(Type::TypeRef(prefix, name));
    assert_eq!(u.resolve_ref(selected), Some(member));
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
    let Some(method) = u.method(info) else { panic!("{shown}") };
    let (mut todo, mut annotations) = (vec![method.params[0].1], Vec::new());
    while let Some(t) = todo.pop() {
        match *u.types().get(t) {
            Type::Annotated(inner, a) => {
                todo.push(inner);
                annotations.push(a);
            }
            Type::Applied(tycon, args) => {
                todo.push(tycon);
                todo.extend(u.types_of(args));
            }
            _ => {}
        }
    }
    let classes: Vec<_> = annotations.into_iter().filter_map(|a| u.class_symbol(a)).collect();
    let classes: Vec<String> = classes.into_iter().map(|c| u.full_name(c)).collect();
    let unchecked = "scala.annotation.unchecked.uncheckedVariance".to_string();
    assert!(classes.contains(&unchecked), "{shown}: {classes:?}");
}

/// An annotated type of a library's TASTy is told from the same type without its annotation tree,
/// which keeps none, and a type map rebuilding it keeps the library's tree.
#[test]
fn a_library_annotated_type_is_told_from_one_without_its_tree() {
    let Some(mut u) = stdlib() else { return };
    let [m] = members(&mut u, "scala.collection", "IterableOps", "fromSpecific")[..] else {
        panic!("one fromSpecific")
    };
    let info = u.info(m);
    let param = u.method(info).expect("a method").params[0].1;
    let (mut todo, mut found) = (vec![param], None);
    while let Some(t) = todo.pop() {
        match *u.types().get(t) {
            Type::Annotated(_, a) if u.types().annotation_tree(t).is_some() => found = Some((t, a)),
            Type::Annotated(inner, _) => todo.push(inner),
            Type::Applied(tycon, args) => {
                todo.push(tycon);
                todo.extend(u.types_of(args));
            }
            _ => {}
        }
    }
    let (library, annot) = found.expect("an annotated type with a library tree");
    let tree = u.types().annotation_tree(library);
    assert!(tree.is_some());

    let own = u.intern_type(*u.types().get(library));
    assert_ne!(own, library);
    assert_eq!(u.types().annotation_tree(own), None);
    assert_eq!(u.carry_annotation_tree(own, library), library);
    assert_eq!(u.carry_annotation_tree(library, own), library);

    let int = u.class_ref("scala.Int");
    let rebuilt = u.intern_type(Type::Annotated(int, annot));
    let carried = u.carry_annotation_tree(library, rebuilt);
    assert_ne!(carried, rebuilt);
    assert_eq!(u.types().annotation_tree(carried), tree);
}

#[test]
fn erasure_of_aliases_arrays_and_type_parameters() {
    let Some(mut u) = stdlib() else { return };
    let erased_result = |u: &mut Universe, pkg, class, name| {
        let m = members(u, pkg, class, name)[0];
        let mut t = u.info(m);
        while let tasty::Type::Method { result, .. } | tasty::Type::Poly { result, .. } = *u.types().get(t) {
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
