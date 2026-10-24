//! Forward references in blocks that extend over a value definition are errors (dotty's
//! `ForwardDepChecks`).

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::dump::{dump, library_jars};

const CASES: &[(&str, bool)] = &[
    ("def f = { val a = summon[Sh[Boolean]]; given Sh[Boolean] = new Sh[Boolean] {}; a }", true),
    ("def f = { val g = () => b; val b = 1; g() }", true),
    ("def f = { def h = b; val x = h; val b = 2; x }", true),
    ("def f = { lazy val a = b; val b = 1; a }", true),
    ("def f = { val b = 1; lazy val a = c; lazy val c = b; a }", false),
    ("def f = { def h: Int = g; def g = 1; h }", false),
    ("def f = { val a = 1; val b = a; b }", false),
];

#[test]
fn forward_references_over_values_are_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let jars = library_jars(&root.join("../../ref/libs"));
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return;
    }
    let mut failures = Vec::new();
    for &(case, error) in CASES {
        let mut u = Universe::open(&jars).expect("the library");
        let text = format!("trait Sh[T]\nobject O {{ {case} }}\n");
        let actual = dump(&mut u, &SourceFile::new("case.scala", text.as_str()), &[], false, true);
        if actual.contains("forward reference to") != error {
            failures.push(format!("{case}: {}", actual.lines().last().unwrap_or_default()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
