//! TASTy pickled beside the NIR on Scala Native.

mod snippet;

use lower::Backend;
use lower::output::{Lowered, nir_with_tasty};
use snippet::jars;
use syntax::config::Config;
use syntax::source::SourceFile;
use tasty::Universe;

fn lowered(sources: &[SourceFile]) -> Option<Lowered> {
    lowered_with(sources, false)
}

/// `sources` lowered with TASTy, the JDK stubs standing for the JDK's class files if `jdk_stubs`.
fn lowered_with(sources: &[SourceFile], jdk_stubs: bool) -> Option<Lowered> {
    let mut jars = jars()?;
    let jdk = jars.len()..jars.len() + usize::from(jdk_stubs);
    if jdk_stubs {
        jars.push(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libs/jdk-stubs.jar"));
    }
    let mut open = || {
        let mut u = Universe::open(&jars).unwrap();
        u.set_jdk_jars(jdk.clone());
        u
    };
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
fn the_constructor_of_any_val_results_in_object_as_dotty_signs_it() {
    let Some(out) = lowered(&[SourceFile::new("v.scala", "class V(val x: Int) extends AnyVal")]) else {
        return;
    };
    let tasty = tasty::Tasty::parse(&out.tasty[0].bytes).expect("our TASTy");
    let signed: Vec<String> = tasty.names.iter().map(|(r, _)| tasty.names.show_signed(r)).collect();
    let any_val = signed.iter().find(|n| n.contains("<init>") && n.contains("scala.AnyVal"));
    assert!(any_val.is_none(), "{signed:?}");
}

#[test]
fn a_jdk_stub_annotation_is_built_as_a_java_annotation_by_its_constructor_name() {
    let source = SourceFile::new("d.scala", "class D { @Deprecated def f: Int = 1 }");
    let Some(out) = lowered_with(&[source], true) else { return };
    let tasty = tasty::Tasty::parse(&out.tasty[0].bytes).expect("our TASTy");
    let signed: Vec<String> = tasty.names.iter().map(|(r, _)| tasty.names.show_signed(r)).collect();
    let deprecated = signed.iter().find(|n| n.contains("<init>") && n.contains("java.lang.Deprecated"));
    assert!(deprecated.is_none(), "{signed:?}");
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
        why.starts_with("pickling a synthetic type tree of a capture set")
            && why.ends_with("at a.scala:4:12"),
        "{why}"
    );
}
