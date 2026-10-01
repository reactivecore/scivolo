//! Sources the lowering cannot translate yet are refused, not translated wrongly: by the typer,
//! or by the lowering naming the construct.

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;

/// Each source and what the refusal names.
const CASES: [(&str, &str); 15] = [
    ("object O { lazy val x = \"a\" }", "lazy values"),
    ("object O { def f(c: Boolean): Any = if c then 1 else \"s\" }", "branches of different erased classes"),
    ("object O { private val x = \"a\" }", "private values used only in the constructor"),
    ("class A(x: String) { println(x); def f = x }", "class parameters used in class body statements"),
    ("object O { val u = () }", "fields of type Unit"),
    ("object O { inline def f(): String = \"a\" }", "inline methods"),
    ("object O { def f(x: => String): String = x }", "by-name parameters"),
    ("object O { def f(a: String)(b: String): String = a }", "several parameter lists"),
    ("object O { def f(): String = { lazy val x = \"a\"; x } }", "local lazy values"),
    ("object O { def f(): String = { var x = \"a\"; x } }", "local variables"),
    ("trait T { val x = \"a\" }", "traits with initialization"),
    (
        "class B; class S extends B; trait T { def f: B = new B() }; trait U extends T { override def f: S = new S() }",
        "bridges in traits",
    ),
    (
        "class B; class S extends B; class P { def f: S = new S() }; trait T { def f: B }; class C extends P with T",
        "bridges for inherited members",
    ),
    (
        "class B; class S extends B; trait T { def f: B }; trait U { def f: S = new S() }; class C extends T with U",
        "bridges for inherited members",
    ),
    (
        "class C extends Product { def canEqual(o: Any) = true; def productArity = 0; \
         def productElement(n: Int): Any = 0 }",
        "library traits",
    ),
];

#[test]
fn constructs_not_lowered_yet_are_refused() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let mut failures = Vec::new();
    for (source, refusal) in CASES {
        let mut u = Universe::open(&jars).unwrap();
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
