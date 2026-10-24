//! The `scala.compiletime` expansions the inliner refuses rather than diverge from dotty.

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::dump::{dump, library_jars};

const PRELUDE: &str = "import scala.compiletime.*\ntrait Show[T] { def show(t: T): String }\n\
    object Show { given Show[Int] = new Show[Int] { def show(t: Int) = \"\" } }\n";

const CASES: &[(&str, &str)] = &[
    (
        "object O { import scala.compiletime.ops.int./; def f = constValue[1 / 0] }",
        "constValue of a type that is not constant",
    ),
    (
        "object O { inline def f: String = { given x: Show[Int] = null; summonInline[Show[Int]].show(1) }; \
         def g = f }",
        "implicit search in an inline body with local implicits",
    ),
    (
        "object O { inline def s[T]: Show[T] = summonInline[Show[T]]; \
         transparent inline def t: Show[Int] = { given x: Show[Int] = null; s[Int] }; def g = t }",
        "implicit search in an inline body with local implicits",
    ),
    (
        "object O { given ls[A](using Show[A]): Show[List[A]] = null; inline def f: Int = \
         summonFrom { case s: Show[Int] => summonInline[Show[List[Int]]].hashCode }; def g = f }",
        "implicit search in an inline body with local implicits",
    ),
    (
        "object O { given ls[A](using Show[A]): Show[List[A]] = null; \
         inline def s[T]: Show[T] = summonInline[Show[T]]; \
         transparent inline def t: Show[List[Int]] = { given x: Show[Int] = null; s[List[Int]] }; def g = t }",
        "implicit search in an inline body with local implicits",
    ),
    (
        "object O { given a: Show[String] = new Show[String] { def show(t: String) = \"\" }; \
         given b: Show[String] = a; inline def f: Boolean = summonFrom { case _: Show[String] => true }; \
         def g = f }",
        "ambiguous implicit arguments",
    ),
    ("object O { inline def f(x: Int): String = codeOf(x); def g(y: Int) = f(y) }", "codeOf an argument"),
    ("object O { def g = summonInline[Show[String]] }", "no implicit argument"),
];

#[test]
fn compiletime_refusals() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let jars = library_jars(&root.join("../../ref/libs"));
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return;
    }
    let mut failures = Vec::new();
    for &(case, refusal) in CASES {
        let mut u = Universe::open(&jars).expect("the library");
        let text = format!("{PRELUDE}{case}\n");
        let actual = dump(&mut u, &SourceFile::new("case.scala", text.as_str()), &[], false, true);
        if !actual.starts_with(&format!("== unsupported {refusal}")) {
            failures.push(format!("{case}: {}", actual.lines().next().unwrap_or_default()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
