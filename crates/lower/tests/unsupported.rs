//! Sources the lowering cannot translate yet are refused, not translated wrongly: by the typer,
//! or by the lowering naming the construct.

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;

/// Each source and what the refusal names.
const CASES: &[(&str, &str)] = &[
    (
        "object O { val v: Any = 1; def f(): Boolean = O.v.isInstanceOf[String | Int] }",
        "type tests of unions or intersections of paths other than names",
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
    (
        "object O { def f[T](x: Any): Boolean = x.isInstanceOf[Array[Array[T]]] }",
        "type tests of generic arrays of arrays",
    ),
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
    ("class A(u: Unit) { def f = u }", "private local fields of type Unit"),
    (
        "class A { object B; def f(x: Any) = x.isInstanceOf[B.type] }",
        "type tests of objects nested in classes",
    ),
    ("abstract class E extends Enumeration { def v = new Val(1) {} }", "library classes nested in classes"),
    ("object O { def f: Tuple.Head[EmptyTuple] = ??? }", "ground match types in type trees"),
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
    (
        "sealed trait S[A]; object S { case class X[A](a: A) extends S[A] }",
        "companions of generic sealed classes",
    ),
    ("object O { def f() = { object L { lazy val a = 1 }; L.a } }", "lazy values in local objects"),
    (
        "trait U { def f: Int ?=> Int }; trait T { def f: Int ?=> Int = 1 }; class X extends U with T",
        "overriding methods with context function results flattened otherwise",
    ),
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
    (
        "import scala.reflect.Selectable.reflectiveSelectable\nclass V(val n: Int) extends AnyVal\n\
         object O { def f(p: { def v: V }) = p.v }",
        "structural member of a value class type",
    ),
    (
        "class S extends Selectable { def applyDynamic(n: String)(args: Any*): Any = 1 }\n\
         object O { def f(p: S { def m(x: Int): Int }) = p.m(1) }",
        "structural call on a Selectable other than reflect.Selectable",
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
