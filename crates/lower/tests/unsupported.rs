//! Sources the lowering cannot translate yet are refused, not translated wrongly: by the typer,
//! or by the lowering naming the construct.

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;

/// Each source and what the refusal names.
const CASES: &[(&str, &str)] = &[
    (
        "object O { def g(): Any = 1; def f(): Boolean = g().isInstanceOf[String | Int] }",
        "type tests of unions or intersections of values other than names",
    ),
    ("object O { def f(x: Any): Boolean = x.isInstanceOf[Tuple] }", "type tests of tuple types"),
    ("object O { def f[T](x: Any): Boolean = x.isInstanceOf[Array[T]] }", "type tests of generic arrays"),
    ("object O { def f(): Boolean = ().isInstanceOf[Unit] }", "type tests of primitive values or of null"),
    (
        "object O { def f(x: Boolean): Int = x.asInstanceOf[Int] }",
        "casts between primitive values or to references",
    ),
    (
        "class U { val a: Unit = (); def f(): Unit = (a: Unit) }",
        "ascriptions to Unit of values other than literals",
    ),
    ("object O { def f(x: Any): Unit = x.asInstanceOf[Unit] }", "casts to Unit of values other than blocks"),
    ("object O { def f(): Unit = null.asInstanceOf[Unit] }", "casts to Unit of values other than blocks"),
    (
        "class U { val a: Unit = (); def f(x: Boolean): Unit = { if x then return a; () } }",
        "returns of Unit values of class members",
    ),
    (
        "object E { def unapplySeq(s: String): Option[(Int, Seq[Int])] = None }\n\
         object O { def f(x: String): Int = x match { case E(a, b) => a; case _ => 0 } }",
        "sequence extractors of a product and a sequence",
    ),
    (
        "class M(val x: Int) extends AnyVal\nclass N(val x: Int) extends AnyVal\n\
         object O { def f(m: M) = m.asInstanceOf[N] }",
        "casts between value classes",
    ),
    ("object O { def f[T](a: Array[T]): T = a(0) }", "operations on generic arrays"),
    (
        "object O { def f(c: Boolean): Any = if c then Array(1) else \"s\" }",
        "branches of arrays and other erased classes",
    ),
    ("class A(private var p: Int) { def f = p }", "private mutable class parameters"),
    ("class A(val u: Unit)", "parameters and private local fields of type Unit"),
    (
        "class A { object B; def f(x: Any) = x.isInstanceOf[B.type] }",
        "type tests of objects nested in classes",
    ),
    ("class A { object B { val f = (x: Int) => x } }", "lambdas and local methods in nested classes"),
    ("abstract class E extends Enumeration { def v = new Val(1) {} }", "library classes nested in classes"),
    (
        "class A { class B }; object O { def f(x: Any) = x match { case _: A#B => 1; case _ => 0 } }",
        "patterns of classes in classes",
    ),
    ("object O { def f[T](t: T) = { class C[U](u: U); new C(t) } }", "generic local classes"),
    ("object O { def f = Tuple1(1).size }", "match type in a library inline body"),
    ("object O { def f() = { trait T; new T {} } }", "local traits"),
    (
        "object O { def f(k: Int) = { def g = k; class C { class D { def m = g } }; val c = new C; new c.D().m } }",
        "classes in local classes calling local methods",
    ),
    ("trait T { var x: Int }", "abstract variables"),
    (
        "object O { def f() = ((x: Int) => { class N { def g = 1 }; new N().g + x })(2) }",
        "applications of a lambda defining classes",
    ),
    (
        "object O { def f(x: Null) = x match { case Some(_) => 1; case _ => 2 } }",
        "matches on values of type Null or Nothing",
    ),
    ("object E extends Exception(\"e\")", "forwarders of objects extending JDK classes"),
    ("sealed trait S; object S { val x = 1 }", "companions of sealed classes"),
    ("object O { def f() = { object L { lazy val a = 1 }; L.a } }", "lazy values in local objects"),
    (
        "object O { def f(k: Int) = { object L { object M { val v = k } }; L.M.v } }",
        "classes in local classes capturing values",
    ),
    (
        "object O { def f() = { class C { val g = (x: Int) => x }; new C().g(1) } }",
        "lambdas and local methods in local classes",
    ),
    (
        "class W(val v: Int) { trait In { def get: Int }; def f = new In { def get = new In { def get = v }.get }.get }",
        "classes in anonymous classes using the enclosing instance",
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
    for &(source, refusal) in CASES {
        let mut u = Universe::open(&jars).unwrap();
        let result = lower::compile(&mut u, &SourceFile::new("case.scala", source), &[]).map_err(|r| r.what);
        match result {
            Err(e) if e.starts_with("the typer does not support") => {}
            Err(e) if e.starts_with("the lowering does not support") && e.contains(refusal) => {}
            Err(e) => failures.push(format!("{source}: {e}")),
            Ok(_) => failures.push(format!("{source}: lowered")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
