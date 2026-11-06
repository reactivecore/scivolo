//! Type parameters where their variance does not allow them are errors (dotty's
//! `VarianceChecker`).

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::dump::{dump, library_jars};

const CASES: &[(&str, bool)] = &[
    ("class C[+A] { def pick[T](x: T, y: A): T = x }", true),
    ("class C[-A] { def f: A = ??? }", true),
    ("class C[+A] { var x: A = ??? }", true),
    ("class C[+A] { type T = A }", true),
    ("class C[+A] { def f[B <: A](b: B): Unit = () }", true),
    ("class C[+A] { class Inner(x: A) }", true),
    ("trait Inv[T]; class C[+A] extends Inv[A]", true),
    ("class C[+A] { def f: { def g(x: A): Unit } = ??? }", true),
    ("class C[-M](private val v: Array[M])", true),
    ("class C[+A] { private def p(a: A) = (); def q(o: C[Int]) = o.p(1) }", true),
    (
        "class C[+A](a: A) { private var c: A = a; def put[B >: A](b: B) = { val o: C[B] = this; o.c = b } }",
        true,
    ),
    ("type F[+X] = X => Int", true),
    ("class C[+A](a: A) { private var c: A = a; private def put(x: A) = c = x; def get: A = c }", false),
    ("class C[+A] { def lower[B >: A](b: B): List[B] = Nil; def fold[B](f: A => B): B = ??? }", false),
    ("class C[+A] { def f(x: A @annotation.unchecked.uncheckedVariance) = (); type T <: A }", false),
    ("class C[+A](a: A) { def f(): Unit = { def g(x: A) = (); class L { def h(y: A) = y } } }", false),
    ("case class C[+A](x: A, ys: List[A]); enum E[+A] { case S(x: A); case N }", false),
    ("class C[-A, +B](f: A => B) { def andThen[Z](g: B => Z): C[A, Z] = new C(a => g(f(a))) }", false),
];

#[test]
fn type_parameters_at_positions_their_variances_forbid_are_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let jars = library_jars(&root.join("../../ref/libs"));
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return;
    }
    let mut failures = Vec::new();
    for &(case, error) in CASES {
        let mut u = Universe::open(&jars).expect("the library");
        let actual = dump(&mut u, &SourceFile::new("case.scala", case), &[], false, true);
        if actual.contains("position in") != error {
            failures.push(format!("{case}: {}", actual.lines().last().unwrap_or_default()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
