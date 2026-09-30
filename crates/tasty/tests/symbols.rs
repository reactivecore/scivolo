use std::path::Path;
use tasty::tags::*;
use tasty::{Kind, SymbolId, Universe};

/// The universe of the Scala standard library, if `ref/libs` has it.
fn stdlib() -> Option<Universe> {
    let jar = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs/scala-library-3.9.0.jar");
    if !jar.exists() {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", jar.display());
        return None;
    }
    Some(Universe::open(&[jar]).unwrap())
}

fn one(u: &mut Universe, owner: SymbolId, name: &str, kind: Kind) -> SymbolId {
    let found: Vec<_> =
        u.decl(owner, name).unwrap().into_iter().filter(|&d| u.symbol(d).kind == kind).collect();
    assert_eq!(found.len(), 1, "{name}: {found:?}");
    found[0]
}

#[test]
fn a_lookup_loads_only_the_file_defining_the_name() {
    let Some(mut u) = stdlib() else { return };
    let immutable = u.package("scala.collection.immutable").unwrap();
    assert!(u.files.is_empty());
    let list = one(&mut u, immutable, "List", Kind::Class);
    assert_eq!(u.files.len(), 1);
    assert_eq!(u.full_name(list), "scala.collection.immutable.List");
    let flags = u.symbol(list).flags;
    assert!(flags.has(SEALED) && flags.has(ABSTRACT));
    let module = one(&mut u, immutable, "List", Kind::Val);
    assert!(u.symbol(module).flags.has(OBJECT));
    one(&mut u, immutable, "List$", Kind::Class);
    assert_eq!(u.files.len(), 1);
}

#[test]
fn classes_have_their_members_and_type_parameters() {
    let Some(mut u) = stdlib() else { return };
    let scala = u.package("scala").unwrap();
    let option = one(&mut u, scala, "Option", Kind::Class);
    one(&mut u, option, "A", Kind::TypeParam);
    let maps = u.decl(option, "map").unwrap();
    assert_eq!(maps.len(), 1);
    assert_eq!(u.symbol(maps[0]).kind, Kind::Def);
    let flags = u.symbol(maps[0]).flags;
    assert!(flags.has(FINAL) && !flags.has(PRIVATE), "{flags:?}");
    let some = one(&mut u, scala, "Some", Kind::Class);
    assert!(u.symbol(some).flags.has(CASE) && u.symbol(some).flags.has(FINAL));
}

#[test]
fn every_file_enters() {
    let Some(mut u) = stdlib() else { return };
    let mut stack = vec![u.root()];
    let mut symbols = 0;
    while let Some(p) = stack.pop() {
        for d in u.decls(p).unwrap() {
            symbols += 1;
            if u.symbol(d).kind == Kind::Package {
                stack.push(d);
            }
        }
    }
    assert_eq!(u.files.len(), 941);
    assert!(symbols > 1000, "{symbols}");
}
