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
    ("object O { def f() = 3.compare(4) }", "members value classes inherit"),
    ("object O { def f[T](a: Array[T]): T = a(0) }", "operations on generic arrays"),
    (
        "object O { def f(c: Boolean): Any = if c then Array(1) else \"s\" }",
        "branches of arrays and other erased classes",
    ),
    ("class A(private var p: Int) { def f = p }", "private mutable class parameters"),
    (
        "class A(val x: Int) { println(this.x) }",
        "overridable class parameters selected on this in the constructor",
    ),
    ("class A(val u: Unit)", "parameters and private local fields of type Unit"),
    (
        "class A { object B; def f(x: Any) = x.isInstanceOf[B.type] }",
        "type tests of objects nested in classes",
    ),
    (
        "class A { class B }; object O { val a = new A; class D { def f = new O.a.B } }",
        "creating an inner class through this prefix",
    ),
    ("class A { object B { val f = (x: Int) => x } }", "lambdas and local methods in nested classes"),
    ("class A { class B; class C extends B }", "subclasses of classes in classes"),
    (
        "class A { class B }; object O { def f(x: Any) = x match { case _: A#B => 1; case _ => 0 } }",
        "patterns of classes in classes",
    ),
    ("object O { def f() = { case class P(x: Int); P(1) } }", "local case classes"),
    ("object O { def f[T](t: T) = { class C[U](u: U); new C(t) } }", "generic local classes"),
    (
        "class A(n: Int) { def f() = { class C { def g = n }; new C() } }",
        "local classes using the enclosing instance",
    ),
    ("object O { inline def f(): String = \"a\" }", "inline methods"),
    (
        "object O { def f(): String = { lazy val x = \"a\"; class C { def g = x }; new C().g } }",
        "local classes calling local methods",
    ),
    ("object O { def f() = { trait T; new T {} } }", "local traits"),
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
        "class B { trait T { def f = 1 } }; object O { def g(b: B) = { class C extends b.T; new C().f } }",
        "local classes using the enclosing instance",
    ),
    (
        "class A(k: Int) { val v = { class C { def g = k }; new C().g } }",
        "local classes using the enclosing instance",
    ),
    (
        "class A(f: () => Int); object O { class B extends A(() => 1) }",
        "superclass constructor arguments defining or creating local classes or methods",
    ),
    (
        "class A[T](f: T)(n: Int); object O { class B extends A[() => Int](() => 1)(2) }",
        "superclass constructor arguments defining or creating local classes or methods",
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
