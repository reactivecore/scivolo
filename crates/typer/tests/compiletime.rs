//! The `scala.compiletime` expansions the inliner refuses rather than diverge from dotty, and
//! the local givens its searches find.

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
        "object O { given a: Show[String] = new Show[String] { def show(t: String) = \"\" }; \
         given b: Show[String] = a; inline def f: Boolean = summonFrom { case _: Show[String] => true }; \
         def g = f }",
        "ambiguous implicit arguments",
    ),
    (
        "object O { inline def f: Int = { class C { given c: Show[Int] = null; def m = summonInline[Show[Int]] }; \
         1 }; def g = f }",
        "implicit search in an inline body with local implicits",
    ),
    ("object O { def g = codeOf(List(1, 2)) }", "codeOf an argument"),
    ("object O { def h(using i: Int) = i; given Int = 3; def g = codeOf(h) }", "codeOf an argument"),
    ("object O { inline def f(x: Int): String = codeOf(x); def g(y: Int) = f(y + 1) }", "codeOf an argument"),
    ("object O { def g(y: Int) = codeOf(`y`) }", "codeOf an argument"),
    ("object O { import scala.Predef.{print as p}; def g = codeOf(p(1)) }", "codeOf an argument"),
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

/// Searches in inline bodies with local givens in scope, and the symbols their expansion refers
/// to in this order.
const LOCAL_GIVENS: &[(&str, &[&str])] = &[
    (
        "object O { inline def s[T]: Show[T] = summonInline[Show[T]]; \
         transparent inline def t: Show[Int] = { given x: Show[Int] = null; s[Int] }; def g = t }",
        &["O$.s#(1)", "O$._$x"],
    ),
    (
        "object O { given ls[A](using Show[A]): Show[List[A]] = null; inline def f: Int = \
         summonFrom { case s: Show[Int] => summonInline[Show[List[Int]]].hashCode }; def g = f }",
        &["O$.ls#(1,Show)", "O$._$s"],
    ),
    (
        "object O { given ls[A](using Show[A]): Show[List[A]] = null; \
         inline def s: Int = summonFrom { case _: Show[List[Long]] => 1; case _ => 2 }; \
         transparent inline def t: Int = { given x: Show[Long] = null; s }; def g = t }",
        &["O$.ls#(1,Show)", "O$._$x", "#Int:\"1\""],
    ),
    (
        "object O { given ls[A](using Show[A]): Show[List[A]] = null; \
         inline def s[T]: Show[T] = summonInline[Show[T]]; \
         transparent inline def t: Show[List[Int]] = { given x: Show[Int] = null; s[List[Int]] }; def g = t }",
        &["O$.ls#(1,Show)", "O$._$x"],
    ),
    (
        "object O { given o: Show[Int] = null; inline def s[T]: Show[T] = summonInline[Show[T]]; \
         transparent inline def t(v: Any): Show[Int] = v match { case (_, given Show[Int]) => s[Int] }; \
         def g = t((1, o)) }",
        &["O$.s#(1)", "O$._$given_Show_Int"],
    ),
    (
        "object O { given o: Show[Int] = null; inline def f(v: Any): Show[Int] = \
         v match { case given Show[Int] => summonInline[Show[Int]] }; def g = f(1) }",
        &["CaseDef", "Block", "O$._$given_Show_Int"],
    ),
    (
        "object O { inline def f: String = { def h(using x: Show[Int]) = summonInline[Show[Int]].show(1); \
         h(using null) }; def g = f }",
        &["O$._$_$x", "Show$.given_Show_Int"],
    ),
    (
        "object O { inline def s[T]: Show[T] = summonInline[Show[T]]; transparent inline def t: Show[Int] = \
         { def h(using x: Show[Int]) = s[Int]; h(using null) }; def g = t }",
        &["O$._$_$x", "O$.s#(1)", "O$._$_$x"],
    ),
];

#[test]
fn local_givens_in_inline_searches() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let jars = library_jars(&root.join("../../ref/libs"));
    if jars.iter().any(|j| !j.exists()) {
        return;
    }
    let mut failures = Vec::new();
    for &(case, refs) in LOCAL_GIVENS {
        let mut u = Universe::open(&jars).expect("the library");
        let text = format!("{PRELUDE}{case}\n");
        let actual = dump(&mut u, &SourceFile::new("case.scala", text.as_str()), &[], false, true);
        let mut rest = actual.as_str();
        let in_order = refs.iter().all(|r| match rest.find(r) {
            Some(i) => {
                rest = &rest[i + r.len()..];
                true
            }
            None => false,
        });
        if !in_order {
            failures.push(format!("{case}: {}", actual.lines().next().unwrap_or_default()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
