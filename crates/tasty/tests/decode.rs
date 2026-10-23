use std::io::Read;
use std::path::Path;
use tasty::{Ast, Cursor, Tasty, dump};

#[test]
fn nat_and_long_int_use_stop_bit_on_last_byte() {
    let mut c = Cursor::new(&[0x81, 0x01, 0x80, 0xff, 0x7f, 0xff]);
    assert_eq!(c.nat().unwrap(), 1);
    assert_eq!(c.nat().unwrap(), 128);
    assert_eq!(c.long_int().unwrap(), -1);
    assert_eq!(c.long_int().unwrap(), -1);
}

/// The dump of fixture `name`, compiled from `tests/fixtures/Sample.scala`.
fn sample(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}.tasty", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(path).unwrap();
    let t = Tasty::parse(&bytes).unwrap();
    let ast = Ast::parse(t.section("ASTs").unwrap().bytes).unwrap();
    assert_eq!(ast.dangling_refs(), 0);
    dump(&ast, &t.names)
}

#[test]
fn sample_files_decode() {
    for f in ["Circle", "Rect", "Shape", "Show", "Sample$package"] {
        sample(f);
    }
    let main = sample("Main");
    assert!(main.contains("sample.Sample$package.twice"));
    assert!(main.contains("INLINED 1 Sample$package$"));
}

#[test]
fn forward_references_are_errors() {
    let mut names = Cursor::new(&[2, 0x82, 0x80, 0x81]);
    assert!(tasty::Names::parse(&mut names, 4).is_err(), "a name referring to itself");
    let shared = [tasty::tags::SHAREDtype, 0x85];
    assert!(Ast::parse(&shared).is_err(), "a shared type referring forward");
}

#[test]
fn standard_library_decodes() {
    let jar = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs/scala-library-3.9.0.jar");
    let Ok(file) = std::fs::File::open(&jar) else {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", jar.display());
        return;
    };
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut files = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let name = entry.name().unwrap().into_owned();
        if !name.ends_with(".tasty") {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        let t = Tasty::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let ast = Ast::parse(t.section("ASTs").unwrap().bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(ast.dangling_refs(), 0, "{name}");
        files += 1;
    }
    assert_eq!(files, 941);
}
