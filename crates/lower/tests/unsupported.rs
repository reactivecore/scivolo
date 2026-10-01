//! Sources the lowering cannot translate yet are refused, not translated wrongly: by the typer,
//! or by the lowering naming the construct.

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;

/// Each source and what the refusal names.
const CASES: [(&str, &str); 7] = [
    ("@main def m(): Unit = println(42)", "boxing"),
    ("object O { inline def f(): String = \"a\" }", "inline methods"),
    ("object O { def f(x: => String): String = x }", "by-name parameters"),
    ("object O { def f(a: String)(b: String): String = a }", "several parameter lists"),
    ("object O { def f(): String = { lazy val x = \"a\"; x } }", "local lazy values"),
    ("object O { def f(): String = { var x = \"a\"; x } }", "local variables"),
    ("object O { def f(): Any = 1 }", "boxing"),
];

#[test]
fn constructs_not_lowered_yet_are_refused() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let mut u = Universe::open(&jars).unwrap();
    let mut failures = Vec::new();
    for (source, refusal) in CASES {
        let result = lower::compile(&mut u, &SourceFile::new("case.scala", source), &[]);
        match result {
            Err(e) if e.starts_with("the typer does not support") => {}
            Err(e) if e.starts_with("the lowering does not support") && e.contains(refusal) => {}
            Err(e) => failures.push(format!("{source}: {e}")),
            Ok(_) => failures.push(format!("{source}: lowered")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
