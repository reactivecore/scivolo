//! TASTy pickled beside the NIR on Scala Native.

mod snippet;

use lower::Backend;
use lower::output::{Lowered, nir_with_tasty};
use snippet::jars;
use syntax::config::Config;
use syntax::source::SourceFile;
use tasty::Universe;

fn lowered(sources: &[SourceFile]) -> Option<Lowered> {
    let jars = jars()?;
    let mut open = || Universe::open(&jars).unwrap();
    let backend = (Backend::NativeWithTasty, nir_with_tasty);
    let isolated = lower::compile_isolated(&mut open, sources, Config::from_options(&[]), None, backend);
    let isolated = isolated.unwrap_or_else(|r| panic!("{}", r.what));
    assert!(isolated.refused.is_empty(), "{}", isolated.refused[0].refusal.what);
    Some(isolated.out)
}

/// The tag of a name table entry that is a UTF-8 string.
const UTF8: u8 = 1;

/// Whether TASTy `bytes` has a UTF8 name `name` of under 128 bytes.
fn has_name(bytes: &[u8], name: &str) -> bool {
    let entry = [&[UTF8, 0x80 | name.len() as u8], name.as_bytes()].concat();
    bytes.windows(entry.len()).any(|w| w == entry)
}

#[test]
fn top_level_classes_are_pickled_under_their_binary_names_with_the_source_root_path() {
    let mut a = SourceFile::new("a.scala", "package p\nclass C\nobject C\nobject O");
    a.tasty_path = Some("module/src/p/a.scala".to_owned());
    let Some(out) = lowered(&[a, SourceFile::new("b.scala", "package q\nclass D")]) else { return };
    let paths: Vec<&str> = out.tasty.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["p/C.tasty", "p/O.tasty", "q/D.tasty"]);
    assert!(has_name(&out.tasty[0].bytes, "module/src/p/a.scala"));
    assert!(
        !has_name(&out.tasty[0].bytes, "a.scala"),
        "the SourceFile annotation names the NIR position name"
    );
}

#[test]
fn a_file_whose_classes_are_not_pickled_keeps_its_nir() {
    let captures =
        "import language.experimental.captureChecking\nclass A\nclass B\ncase class K(a: A^, b: B^{a})";
    let sources = [SourceFile::new("a.scala", captures), SourceFile::new("b.scala", "class D")];
    let Some(out) = lowered(&sources) else { return };
    let names: Vec<&str> = out.nir.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"K") && names.contains(&"D"), "{names:?}");
    let paths: Vec<&str> = out.tasty.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["D.tasty"]);
    let [why] = &out.unpickled[..] else { panic!("{:?}", out.unpickled) };
    assert!(
        why.starts_with("pickling a synthetic type tree of a capture set") && why.ends_with("at a.scala:4:1"),
        "{why}"
    );
}
