use std::io::Write;
use std::path::{Path, PathBuf};
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
    let opened = u.loaded_files();
    assert!(opened <= 1, "open loads only AnyVal's file");
    let list = one(&mut u, immutable, "List", Kind::Class);
    assert_eq!(u.loaded_files(), opened + 1);
    assert_eq!(u.full_name(list), "scala.collection.immutable.List");
    let flags = u.symbol(list).flags;
    assert!(flags.has(SEALED) && flags.has(ABSTRACT));
    let module = one(&mut u, immutable, "List", Kind::Val);
    assert!(u.symbol(module).flags.has(OBJECT));
    one(&mut u, immutable, "List$", Kind::Class);
    assert_eq!(u.loaded_files(), opened + 1);
}

#[test]
fn names_whose_files_are_encoded_or_module_classes() {
    let Some(mut u) = stdlib() else { return };
    let immutable = u.package("scala.collection.immutable").unwrap();
    let cons = one(&mut u, immutable, "::", Kind::Class);
    assert!(u.symbol(cons).flags.has(CASE));
    let runtime = u.package("scala.runtime").unwrap();
    one(&mut u, runtime, "Nothing$", Kind::Class);
}

#[test]
fn classes_have_their_members_type_parameters_and_parameters() {
    let Some(mut u) = stdlib() else { return };
    let scala = u.package("scala").unwrap();
    let option = one(&mut u, scala, "Option", Kind::Class);
    one(&mut u, option, "A", Kind::TypeParam);
    let map = one(&mut u, option, "map", Kind::Def);
    let flags = u.symbol(map).flags;
    assert!(flags.has(FINAL) && !flags.has(PRIVATE), "{flags:?}");
    let some = one(&mut u, scala, "Some", Kind::Class);
    assert!(u.symbol(some).flags.has(CASE) && u.symbol(some).flags.has(FINAL));
    let value = one(&mut u, some, "value", Kind::Param);
    assert!(u.symbol(value).flags.has(CASEaccessor));
}

#[test]
fn top_level_definitions_are_found_in_package_objects() {
    let Some(mut u) = stdlib() else { return };
    let compiletime = u.package("scala.compiletime").unwrap();
    one(&mut u, compiletime, "erasedValue", Kind::Def);
    let scala = u.package("scala").unwrap();
    let seq = one(&mut u, scala, "Seq", Kind::Type);
    assert_eq!(u.full_name(u.symbol(seq).owner.unwrap()), "scala.package$");
}

#[test]
fn every_file_enters_into_its_package() {
    let Some(mut u) = stdlib() else { return };
    let (mut done, mut stack) = (Vec::new(), vec![u.root()]);
    while let Some(p) = stack.pop() {
        done.push(p);
        for d in u.decls(p).unwrap() {
            let s = u.symbol(d);
            assert!(!matches!(s.name.as_str(), "<init>" | "<empty>"), "{}", u.full_name(d));
            if s.kind == Kind::Package && !done.contains(&d) && !stack.contains(&d) {
                stack.push(d);
            }
        }
    }
    assert_eq!(u.loaded_files(), 941);
}

/// A jar with `entries` under `CARGO_TARGET_TMPDIR`.
fn jar(name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (entry, bytes) in entries {
        zip.start_file(*entry, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    path
}

#[test]
fn a_corrupt_file_is_reported_each_time_and_others_still_load() {
    let good =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Circle.tasty")).unwrap();
    let jar = jar("corrupt.jar", &[("p/Bad.tasty", b"not tasty"), ("p/Circle.tasty", &good)]);
    let mut u = Universe::open(&[jar]).unwrap();
    let p = u.package("p").unwrap();
    assert!(u.decl(p, "Bad").is_err());
    assert!(u.decl(p, "Bad").is_err());
    u.decl(p, "Circle").unwrap();
    assert_eq!(u.loaded_files(), 1);
    assert!(u.decls(p).is_err());
}
