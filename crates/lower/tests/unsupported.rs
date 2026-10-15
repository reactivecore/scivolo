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
    (
        "object W\nclass V(val n: Int) extends AnyVal { def f(x: Any) = x match { case _: (this.type | W.type) => 1; case _ => 0 } }",
        "type tests of this.type of value classes",
    ),
    (
        "class V(val n: Int) extends AnyVal { def f(x: Any) = x.isInstanceOf[this.type] }",
        "type tests of this.type of value classes",
    ),
    (
        "class V(val n: Int) extends AnyVal { type S = this.type; def f(x: Any) = x match { case _: S => 1; case _ => 0 } }",
        "type tests of this.type of value classes",
    ),
    ("object O { def f[T](x: Any): Boolean = x.isInstanceOf[Array[T]] }", "type tests of generic arrays"),
    ("object O { def f(): Boolean = ().isInstanceOf[Unit] }", "type tests of primitive values or of null"),
    (
        "object O { def f(x: Boolean): Int = x.asInstanceOf[Int] }",
        "casts between primitive values or to references",
    ),
    (
        "class U { val a: Unit = (); def f(x: Boolean): Unit = { if x then return a; () } }",
        "returns of Unit values of class members",
    ),
    (
        "final class Odd(s: String) { def lengthCompare(n: Int): Boolean = true; def length: Int = 1;\n\
         def apply(i: Int): Char = 'a'; def drop(n: Int): Seq[Char] = Nil; def toSeq: Seq[Char] = Nil }\n\
         object W { def unapplySeq(s: String): Odd = Odd(s) }\n\
         object O { def f(s: String) = s match { case W(a) => 1; case _ => 0 } }",
        "sequence matches whose `lengthCompare` is no `Int`",
    ),
    (
        "trait Tag\nobject E { def unapplySeq(x: Int): Option[Tag & Seq[Int]] = None }\n\
         object O { def f(x: Int): Int = x match { case E(a) => a; case _ => 0 } }",
        "sequence extractors of another result type",
    ),
    (
        "class M(val x: Int) extends AnyVal\nclass N(val x: Int) extends AnyVal\n\
         object O { def f(m: M) = m.asInstanceOf[N] }",
        "casts between value classes",
    ),
    (
        "object O { def f(c: Boolean): Any = if c then Array(1) else \"s\" }",
        "branches of arrays and other erased classes",
    ),
    ("class A(val u: Unit)", "parameters and private local fields of type Unit"),
    (
        "class A { object B; def f(x: Any) = x.isInstanceOf[B.type] }",
        "type tests of objects nested in classes",
    ),
    ("abstract class E extends Enumeration { def v = new Val(1) {} }", "library classes nested in classes"),
    ("object O { def f[T](t: T) = { class C[U](u: U); new C(t) } }", "generic local classes"),
    ("object O { def f = Tuple1(1).size }", "match type in a library inline body"),
    (
        "object O { def f(k: Int) = { trait T { val v = k }; new T {}.v } }",
        "local traits initializing with captured values",
    ),
    (
        "object O { def f(k: Int) = { trait T { lazy val v = k }; new T {}.v } }",
        "members of local traits other than methods capturing values",
    ),
    (
        "object O { def f(k: Int) = { trait T { def g[A]: Int = k }; new T {}.g[String] } }",
        "references to generic methods of local traits",
    ),
    (
        "object O { def f(k: Int) = { def g = k; class C { class D { def m = g } }; val c = new C; new c.D().m } }",
        "classes in local classes calling local methods",
    ),
    ("trait T { var x: Int }", "abstract variables"),
    ("object E extends Exception(\"e\")", "forwarders of objects extending JDK classes"),
    ("sealed trait S; object S { val x = 1 }", "companions of sealed classes"),
    ("object O { def f() = { object L { lazy val a = 1 }; L.a } }", "lazy values in local objects"),
    (
        "object O { def f(k: Int) = { object L { object M { val v = k } }; L.M.v } }",
        "classes in local classes capturing values",
    ),
    (
        "object O { def f(k: Int) = { class C { def g = { class D { def h = List(1).map(_ + k) }; new D().h } }; new C().g } }",
        "lambdas reading values of enclosing local classes",
    ),
    (
        "class W(val v: Int) { trait In { def get: Int }; def f = new In { def get = new In { def get = v }.get }.get }",
        "classes in anonymous classes using the enclosing instance",
    ),
    (
        "class A { val x: Int = scala.compiletime.uninitialized }",
        "`uninitialized` other than as a variable field's value",
    ),
    (
        "object O { def f() = { var y: Int = scala.compiletime.uninitialized; y } }",
        "`uninitialized` other than as a variable field's value",
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
