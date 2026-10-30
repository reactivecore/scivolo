use std::path::Path;
use tasty::{Kind, SymbolId, Universe};

/// The standard library with the TASTy of javalib (checked in), nativelib, clib and posixlib, if
/// `ref/libs` has them.
fn with_javalib() -> Option<Universe> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fetched = [
        "scala-library-3.9.0.jar",
        "nativelib_native0.5_3-0.5.12.jar",
        "clib_native0.5_3-0.5.12.jar",
        "posixlib_native0.5_3-0.5.12.jar",
    ]
    .map(|j| root.join("ref/libs").join(j));
    if let Some(missing) = fetched.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    let mut jars = fetched.to_vec();
    jars.push(root.join("libs/javalib-tasty-0.5.12.jar"));
    Some(Universe::open(&jars).unwrap())
}

fn class(u: &mut Universe, pkg: &str, name: &str) -> SymbolId {
    let p = u.package(pkg).unwrap();
    let found = u.decl(p, name).unwrap().into_iter().find(|&c| u.symbol(c).kind == Kind::Class);
    found.unwrap_or_else(|| panic!("{pkg}.{name} not found"))
}

#[test]
fn jdk_classes_under_their_jdk_names() {
    let Some(mut u) = with_javalib() else { return };
    let string = class(&mut u, "java.lang", "String");
    assert_eq!(u.full_name(string), "java.lang.String");
    assert_eq!(class(&mut u, "java.lang", "_String"), string);
    assert!(!u.member(string, "length").is_empty());
    let object = class(&mut u, "java.lang", "Object");
    assert_eq!(u.full_name(object), "java.lang.Object");
    assert!(!u.member(object, "hashCode").is_empty());
    assert_eq!(u.member(object, "wait").len(), 3);
    let serializable = class(&mut u, "java.io", "Serializable");
    let some = class(&mut u, "scala", "Some");
    assert!(u.derives_from(some, serializable));
    let array_list = class(&mut u, "java.util", "ArrayList");
    let list = class(&mut u, "java.util", "List");
    assert!(u.derives_from(array_list, list));
}

#[test]
fn renamed_classes_are_referenced_in_their_jdk_package() {
    let Some(mut u) = with_javalib() else { return };
    let class = class(&mut u, "java.lang", "Class");
    let component = u.member(class, "getComponentType")[0];
    let info = u.info(component);
    assert_eq!(u.show(info), "(): Class[>: scala.Nothing <: scala.Any]");
}

#[test]
fn extern_objects_are_marked() {
    let Some(mut u) = with_javalib() else { return };
    let net = u.package("java.net").unwrap();
    let ip = class(&mut u, "java.net", "ip$");
    assert!(u.symbol(ip).is_extern);
    assert!(u.decl(net, "ip").unwrap().into_iter().all(|s| u.symbol(s).is_extern));
    let string = class(&mut u, "java.lang", "String");
    assert!(!u.symbol(string).is_extern);
}
