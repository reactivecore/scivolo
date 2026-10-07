//! Sources the lowering cannot translate yet are refused, not translated wrongly: by the typer,
//! or by the lowering naming the construct.

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;

/// Each source and what the refusal names.
const CASES: &[(&str, &str)] = &[
    (
        "object O { def f(x: Any): Boolean = x.isInstanceOf[String | Int] }",
        "type tests of unions or intersections",
    ),
    ("object O { def f(x: Any): Boolean = x.isInstanceOf[Tuple] }", "type tests of tuple types"),
    (
        "object O { def f(x: Int): Long = x.asInstanceOf[Long] }",
        "casts between primitive values or to references",
    ),
    (
        "object O { def f(x: List[Int]): Int = x match { case List(a) => a; case _ => 0 } }",
        "sequence extractors",
    ),
    (
        "object O { def f(p: String) = StringContext(p, \"y\").s(1) }",
        "interpolations of parts that are not literals",
    ),
    ("object O { def f() = 3.compare(4) }", "members value classes inherit"),
    ("object O { def f[T](a: Array[T]): T = a(0) }", "operations on generic arrays"),
    (
        "object O { def f(c: Boolean): Any = if c then Array(1) else \"s\" }",
        "branches of arrays and other erased classes",
    ),
    ("object O { private val x = \"a\"; println(x) }", "private values used only in the constructor"),
    ("class A(var p: Int) { println(p) }", "mutable class parameters"),
    (
        "class A(val x: Int) { println(this.x) }",
        "overridable class parameters selected on this in the constructor",
    ),
    ("class A(val u: Unit)", "parameters and private local fields of type Unit"),
    ("class A { object B }", "objects in classes"),
    ("class A { class B }", "classes in classes and traits"),
    ("object O { def f() = { case class P(x: Int); P(1) } }", "local case classes"),
    ("object O { def f[T](t: T) = { class C[U](u: U); new C(t) } }", "generic local classes"),
    (
        "class A(n: Int) { def f() = { class C { def g = n }; new C() } }",
        "local classes using the enclosing instance",
    ),
    (
        "abstract class A { def u: Unit }; class B extends A { val u: Unit = () }",
        "values of type Unit overriding",
    ),
    ("object O { inline def f(): String = \"a\" }", "inline methods"),
    ("object O { def f(x: => String): String = x }", "by-name parameters"),
    ("object O { def f(): String = { lazy val x = \"a\"; x } }", "local lazy values"),
    ("trait T { val x = \"a\" }", "traits with initialization"),
    (
        "class B; class S extends B; trait T { def f: B = new B() }; trait U extends T { override def f: S = new S() }",
        "bridges in traits",
    ),
    ("case class C[T](x: T)", "generic case classes"),
    (
        "class V(val n: Int) extends Ordered[V] { def compare(that: V) = n - that.n }",
        "generic library traits",
    ),
    ("sealed trait S; object S { val x = 1 }", "companions of sealed classes"),
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
