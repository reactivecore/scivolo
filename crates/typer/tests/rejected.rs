//! Programs dotty rejects, beside the like programs it accepts.

use std::path::{Path, PathBuf};
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::names::Names;
use syntax::parse::Parser;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::dump::library_jars;
use typer::{Fallback, Units};

fn universe() -> Option<Universe> {
    let libs = std::env::var_os("SCIVOLO_LIBS")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs"));
    let jars = library_jars(&libs);
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    Some(Universe::open(&jars).expect("the library"))
}

/// The outcome of typing `text` alone with errors taking precedence over unsupported constructs.
fn outcome(text: &str) -> Result<(), String> {
    let Some(mut u) = universe() else { return Ok(()) };
    let u = &mut u;
    let source = SourceFile::new("main.scala", text);
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::from_options(&[]));
    let root = p.parse();
    let cfg = p.s.cfg;
    let root = Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, &source, cfg).unit(root);
    assert!(!p.s.diags.has_errors(), "syntax errors in {text}");
    let typed = Units::errors_first().type_unit(u, &mut p.ast, (p.s.names, &mut p.fresh), (root, cfg), None);
    match typed {
        Ok(_) => Ok(()),
        Err(Fallback::Unsupported { what, .. }) => Err(format!("unsupported {what}")),
        Err(Fallback::Errors(d)) => {
            Err(d.unique().iter().map(|d| d.message.as_str()).collect::<Vec<_>>().join("\n"))
        }
    }
}

/// Checks that each of `rejected`, a program and a part of its error, fails with that error and
/// that each of `accepted` is typed.
fn check(rejected: &[(&str, &str)], accepted: &[&str]) {
    if universe().is_none() {
        return;
    }
    for (text, error) in rejected {
        let got = outcome(text).expect_err(text);
        assert!(got.contains(error), "{text}: {got}");
    }
    for text in accepted {
        assert_eq!(outcome(text), Ok(()), "{text}");
    }
}

#[test]
fn local_class_vars_in_the_type_of_a_block() {
    check(
        &[(
            "import scala.language.reflectiveCalls
trait Foo
def f: Int =
  val x = new Foo { var i: Int = 0 }
  x.i\n",
            "value i is not a member of Foo",
        )],
        &[
            "trait Foo extends reflect.Selectable
def f: Unit =
  val x = new Foo { var i: Int = 0 }
  x.i = 1\n",
            "trait Foo extends reflect.Selectable
def f: Int =
  val x = new Foo { var i: Int = 0 }
  x.i\n",
            "trait Foo extends reflect.Selectable
def f: Int =
  val x = new Foo { val i: Int = 0 }
  x.i\n",
        ],
    );
}

#[test]
fn structural_members_of_local_values_aliasing_paths_in_the_type_of_a_block() {
    check(
        &[],
        &[
            "trait Foo extends reflect.Selectable
val y = new Foo { val i: Int = 7 }
def f =
  val x: y.type = y
  x.i\n",
            "trait Foo extends reflect.Selectable
val y = new Foo { val i: Int = 7 }
def f =
  val x = new Foo { val z: y.type = y }
  x.z\n",
        ],
    );
}

#[test]
fn members_implemented_with_dependent_parameters() {
    check(
        &[],
        &["trait A {
  type D >: Null <: C
  def foo(d: D)(d2: d.type): Unit
  trait C
}
object B extends A {
  class D extends C
  def foo(d: D)(d2: d.type): Unit = ()
}\n"],
    );
}

#[test]
fn overriding_results_of_parameter_singleton_types() {
    check(
        &[],
        &["object I extends (Int => Int) {
  def apply(i: Int): i.type = i
}\n"],
    );
}

#[test]
fn f_bounded_wildcard_parameters_stay_abstract() {
    check(
        &[(
            "trait Template[+T <: Template[T]]:
  type Clone <: T { type Clone = Template.this.Clone }
  val self: Clone
def eat[F <: Template[F]](food: F): F = food.self.self
val food: Template[?] = ???
def test = eat(food.self)\n",
            "adaptation",
        )],
        &[],
    );
}

#[test]
fn java_methods_implemented_with_object_for_any() {
    check(
        &[],
        &["class S[A] extends java.util.AbstractSet[A] {
  override def contains(o: Object) = false
  def size = 0
  def iterator: java.util.Iterator[A] = ???
}\n"],
    );
}

#[test]
fn implementations_with_other_clauses_access_or_java_parameters() {
    let incompatible = "override with an incompatible type";
    check(
        &[
            (
                "trait A { def f(using x: Int): Int }\nclass B extends A { def f(x: Int): Int = x }\n",
                incompatible,
            ),
            (
                "trait A { def f(implicit x: Int): Int }\nclass B extends A { def f(x: Int): Int = x }\n",
                incompatible,
            ),
            ("trait A { def f[D[X]]: Int }\nclass B extends A { def f[D[+X]]: Int = 1 }\n", incompatible),
            (
                "trait A { def f: Int }\nclass P { protected def f: Int = 1 }\nclass B extends P with A\n",
                "override with weaker access",
            ),
            (
                "class C extends Comparable[Any] { def compareTo(o: Object): Int = 7 }\n",
                "concrete class with an unimplemented member",
            ),
            (
                "trait TB { def f[A <: AnyRef](x: A): A }\nclass C extends TB { def f[A <: String](x: A): A = x }\n",
                "concrete class with an unimplemented member",
            ),
        ],
        &[
            "trait A { def f(using x: Int): Int }\nclass B extends A { def f(using x: Int): Int = x }\n",
            "trait A { def f(x: Int): Int }\nclass B extends A { def f(using x: Int): Int = x }\n",
            "trait A { def f[D[+X]]: Int }\nclass B extends A { def f[D[X]]: Int = 3 }\n",
            "trait A { def f: Int }\nclass P { def f: Int = 4 }\nclass B extends P with A\n",
            "trait A { protected def f: Int }\nclass P { protected def f: Int = 5 }\nclass B extends P with A { def g = f }\n",
            "trait A { def f(x: Int*): Int }\nclass B extends A { def f(x: Seq[Int]): Int = x.sum }\n",
            "class C extends Comparable[String] { def compareTo(o: String): Int = 7 }\n",
            "abstract class L[+T] { def append[E >: T](that: => L[E]): L[E] }
class Push[+T] extends L[T] {
  override def append[E >: T](that: => L[E]): L[E] =
    new Push[E] { override def append[E2 >: E](that2: => L[E2]): L[E2] = that2 }
}\n",
            "trait Cmp[T <: Cmp[T]] { this: T =>
  def < (that: T): Boolean
  def > (that: T): Boolean = that < this
}
class A(val x: String) extends Cmp[A] { def < (that: A) = this.x < that.x }\n",
        ],
    );
}

#[test]
fn classes_with_self_types_implemented_and_instantiated() {
    check(
        &[("trait T\nclass C { self: T => }\ndef f = new C\n", "not conforming to its self type")],
        &["trait Graph {
  type Node <: NodeIntf
  trait NodeIntf { def connectWith(node: Node): Int }
}
abstract class DirectedGraph extends Graph {
  class NodeImpl extends NodeIntf { self: Node =>
    def connectWith(node: Node): Int = 1
  }
}
class ConcreteGraph extends DirectedGraph {
  type Node = NodeImpl
  def newNode: Node = new NodeImpl
}\n"],
    );
}

#[test]
fn overrides_through_self_types_default_getters_and_unchecked_override() {
    check(
        &[(
            "trait A { def f = 1 }\ntrait B { def f = 2 }\nclass C extends A, B\n",
            "conflicting inherited members",
        )],
        &[
            "trait TCommon { def f: String }
class C1 extends TCommon { def f = \"in C1\" }
trait TOverrider { this: TCommon =>
  override def f = \"in TOverrider\"
}
class C2 extends C1 with TOverrider\n",
            "abstract class FileOps { def withLock[R](start: Long = 0): Option[R] }
trait DefaultFileOps { self: DefaultPath =>
  override def withLock[R](start: Long = 5): Option[R] = None
}
class DefaultPath extends FileOps with DefaultFileOps\n",
            "import scala.annotation.unchecked.uncheckedOverride
trait Base[T] { def max[U <: T](x: U, y: U): U = x; def mux(x: String = \"ex\") = x }
trait Sub[T] extends Base[T] {
  @uncheckedOverride def max[U <: T](x: U, y: U): U = x
  @uncheckedOverride def min[U <: T](x: U, y: U): U = x
  @uncheckedOverride def mux(x: String = \"nox\") = x
}\n",
        ],
    );
}

#[test]
fn private_definitions_leaking_through_signatures() {
    check(
        &[
            (
                "class O { private val x: Int = 1\n  class I { def f: x.type = x } }\n",
                "non-private method f in class I refers to private value x",
            ),
            (
                "class O { private class P\n  def f(p: P) = 1 }\n",
                "non-private method f in class O refers to private class P",
            ),
            (
                "trait A { private class P\n  class I[T <: P] }\n",
                "non-private constructor I in class I refers to private class P",
            ),
            (
                "class O { private class P\n  class Q extends P }\n",
                "non-private class Q in class O refers to private class P",
            ),
            (
                "object S { private type T\n  object A { private[S] def b: T = ??? }\n  export A.* }\n",
                "non-private method b in object S refers to private type T",
            ),
        ],
        &[
            "class O { private type T = Int\n  def f: T = 1\n  val g: List[T] = Nil }\n",
            "class O { private class P\n  private def f: P = new P\n  private[this] val g = f }\n",
            "class O { private class P\n  class I { private def f: P = new P } }\n",
            "object O { private class P\n  def f: Int = { val p = new P; 1 } }\n",
            "class O[T] { private val x: T = ???\n  def f: T = x }\n",
            "object O { private object M { class X; val x = 1 }\n  def f: M.X = new M.X\n  def g: M.x.type = M.x }\n",
        ],
    );
}

#[test]
fn members_against_refinements() {
    check(
        &[
            ("def f(x: Any { def m: Int }): Any { val m: Int } = x\n", ""),
            ("class C { def m: Int = 1 }\ndef f(x: C): Any { val m: Int } = x\n", ""),
            ("class C { var m: Int = 1 }\ndef f(x: C): Any { val m: Int } = x\n", ""),
            (
                "import scala.annotation.targetName\nclass C { @targetName(\"n\") def m(): Unit = () }\n\
                 def f(x: C): Any { def m(): Unit } = x\n",
                "",
            ),
            ("class C[X] { def m(x: X): Int = 1 }\nval v: { def m(x: String): Int } = new C[String]\n", ""),
        ],
        &[
            "def f(x: Any { val m: Int }): Any { def m: Int } = x\n",
            "class C { val m: Int = 1 }\ndef f(x: C): Any { val m: Int } = x\n",
            "class C { var m: Int = 1 }\ndef f(x: C): Any { def m: Int } = x\n",
            "class C { def m(x: String): Int = 1 }\nval v: { def m(x: String): Int } = new C\n",
            "val b: { def length: Int } = \"s\"\n",
            "val b: { def length(): Int } = \"s\"\n",
            "val f: Function1[Int, Int] { def apply(x: Int): Int } = x => x\n",
            "val g: (Object & Comparable[String]) { def toString: String } = \"s\"\n",
            "class C { def m(): Int = 1 }\nval a: { def m: Int } = new C\n",
            "class C { override def toString() = \"c\" }\nval a: { def toString: String } = new C\n",
            "class D[X] { def m(x: X): Int = 1 }\nclass A[T <: { def m(x: String): Int }]\n\
             val a = new A[D[String]]\n",
            "class D[X] { def m(x: X): Int = 1 }\ntrait S { self: { def m(x: String): Int } => }\n\
             class E extends D[String] with S\n",
        ],
    );
}

#[test]
fn by_name_function_parameters_are_not_values() {
    check(
        &[("val a: Int => Int = x => x\nval b: (=> Int) => Int = a\n", "")],
        &["val a: (=> Int) => Int = x => x\nval b: (=> Int) => Any = a\nval c: Int => Int = x => x\n"],
    );
}

#[test]
fn assignments_through_wildcard_prefixes() {
    check(
        &[("class B[T](var v: T)\ndef f(s: B[String]) =\n  var b: B[?] = s\n  val x = b.v\n  b.v = x\n", "")],
        &[
            "class B[T](var v: T)\ndef f(b: B[?]) = b.v = b.v\n",
            "class B[T](var v: T)\ndef f(s: B[Any]) =\n  var b: B[? >: String] = s\n  b.v = \"a\"\n",
        ],
    );
}

#[test]
fn conversions_to_object_or_null_are_not_searched() {
    check(
        &[("class W\ngiven Conversion[Any, W] = _ => W()\ndef f[T](x: AnyRef, y: T) = x eq y\n", "")],
        &["def f[T <: AnyRef](x: T, y: T) = x eq y\n"],
    );
}

#[test]
fn comparisons_with_null_of_a_value_type_argument() {
    check(
        &[
            (
                "class R[T <: AnyRef] { val u: R[T] = this; def get: T = ??? }\n\
                 def f[T <: AnyVal](r: R[T @unchecked]) = r.u.get != null\n",
                "",
            ),
            ("def f[T <: AnyVal](w: scala.ref.WeakReference[T]) = w.underlying.get != null\n", ""),
        ],
        &[
            "class R[T <: AnyRef] { def get: T = ??? }\ndef f[T <: AnyRef](r: R[T]) = r.get != null\n",
            "class Q[E <: AnyRef] { def this(c: java.util.Collection[? <: E]) = { this(); \
             val e = c.iterator().next(); if e == null then throw new Exception } }\n",
        ],
    );
}

#[test]
fn inferred_types_selecting_a_missing_member() {
    check(
        &[(
            "trait P { type A }\ntrait Q { type A <: P; def f(a: A): a.A }\n\
             object I extends Q { type A = Nothing; def f(a: A) = a }\n",
            "a.A is not defined in inferred type a.A",
        )],
        &["trait P { type A }\ntrait Q { def f(a: P): a.A }\nobject I extends Q { def f(a: P) = ??? }\n"],
    );
}

#[test]
fn private_members_out_of_reach() {
    check(
        &[
            (
                "import scala.language.implicitConversions\nclass E[L](val l: L)\n\
                 class G { class N private[G](val i: Int) }\n\
                 object S { implicit def c[T](x: T): E[T] = new E(x)\n  object g extends G\n  \
                 import g.N\n  val m = new N(1) }\n",
                "inaccessible member",
            ),
            (
                "trait S { private type V = String\n  case class C(v: List[V]) }\n\
                 object O { new S { val c = C(List(\"1\")) } }\n",
                "",
            ),
            (
                "trait S { private type V = String\n  case class C(v: List[V]) }\nclass Sub extends S\n\
                 def f(s: Sub) = s.C(List(\"1\"))\n",
                "",
            ),
        ],
        &[
            "trait S { private type V = String\n  case class C(v: List[V])\n  val c = C(List(\"1\")) }\n",
            "trait S { private type V = String\n  case class C(v: List[V]) }\ndef f(s: S) = s.C(List(\"1\"))\n",
        ],
    );
}

#[test]
fn overrides_with_higher_kinded_or_refined_types() {
    let error = "unsupported override with an incompatible type";
    check(
        &[
            ("class A { type F[X] <: List[X] }\nclass B extends A { type F[X] = Option[X] }\n", error),
            ("class A { type T <: AnyRef { def m: Int } }\nclass B extends A { type T = String }\n", error),
            (
                "class H[F[_]]\nclass A { def f: H[[x] =>> List[x]] = ??? }\n\
                 class B extends A { override def f: H[[x] =>> Option[x]] = ??? }\n",
                error,
            ),
            (
                "class I[T]\nclass A { type F[X <: A] <: I[A] }\nclass B extends A { type F[X <: A] = I[X] }\n",
                error,
            ),
            (
                "trait O { def w: O.F[Int] = ??? }\nobject O { class F[T] }\n\
                 trait S extends O { override def w: S.F[Int] = ??? }\nobject S { class F[T] }\n",
                error,
            ),
        ],
        &[
            "class A { type F[G[X] <: List[X]] <: Seq[Int] }\nclass B extends A { type F[G[X] <: List[X]] = G[Int] }\n",
            "trait O { def w: O.F[Int] = ??? }\nobject O { class F[T] }\n\
             trait S extends O { override def w: S.F[Int] = ??? }\nobject S { class F[T] extends O.F[T] }\n",
            "class A { type F[X] <: Seq[X] }\nclass B extends A { type F[X] = List[X] }\n",
            "class A { type F[X <: A] <: A }\nclass B extends A { type F[X <: A] = X }\n",
            "class A { class N; type T <: N }\nclass B extends A { class M extends super.N; type T = M }\n",
            "class A { type T <: AnyRef { def length(): Int } }\nclass B extends A { type T = String }\n",
            "class H[F[_]]\nclass A { def f: H[[x] =>> List[x]] = ??? }\n\
             class B extends A { override def f: H[[y] =>> List[y]] = ??? }\n",
            "trait L { type S <: L; type M <: L { type M = L.this.S } }\n\
             trait I extends L { type S = I; type M = E }\ntrait E extends L { type S = E; type M = I }\n",
        ],
    );
}

#[test]
fn types_bounded_by_arrays_of_themselves() {
    check(
        &[
            ("class C[F[x] <: Array[F[x]]]\n", "type bounded by an array of itself"),
            ("class C[T <: Array[T]]\n", "type bounded by an array of itself"),
            ("class C[T <: Array[Array[T]] | Null]\n", "type bounded by an array of itself"),
            ("def f[T <: Array[T]](x: T) = x\n", "type bounded by an array of itself"),
        ],
        &[
            "class C[T <: Array[Int]]\n",
            "class C[T <: Comparable[T]]\n",
            "class C[T, U <: Array[T]]\n",
            "trait C { type T <: Array[Array[T]] }\n",
        ],
    );
}

#[test]
fn erasing_a_type_bounded_by_an_array_of_itself_is_refused() {
    let Some(mut u) = universe() else { return };
    let source = SourceFile::new("main.scala", "trait C { type T <: Array[T]; def f(x: T): T }\n");
    let dumped = typer::dump::dump(&mut u, &source, &[], true, false);
    assert!(dumped.contains(tasty::ARRAY_OF_ITSELF), "{dumped}");
}

#[test]
fn checks_of_single_constructs() {
    check(
        &[
            ("class A[T]\nclass B extends A[?]\n", "Type argument must be fully defined"),
            ("class C[?]\n", "`?` is not a valid type name"),
            ("class C[M[?]]\n", "`?` is not a valid type name, use `_`"),
            ("def f[?] = 1\n", "`?` is not a valid type name"),
            ("class L[T] { class P }\nobject O { def f(x: L#P) = 1 }\n", "Required: ?{ P: ? }"),
            (
                "trait T { def m = 1 }\nobject O extends T { inline def i = super[T].m }\n",
                "Super call not allowed in inlineable method i",
            ),
            (
                "trait T { type X = Int }\nclass O extends T { inline def i: super.X = 1 }\n",
                "Super call not allowed in inlineable method i",
            ),
            (
                "import language.experimental.saferExceptions\nclass E extends Exception\n\
                 def f(): Int = try 1 catch { case _: E if false => 2 }\n",
                "if no pattern guard is given",
            ),
            (
                "import language.experimental.saferExceptions\ncase class E(i: Int) extends Exception\n\
                 def f(): Int = try 1 catch { case E(_) => 2 }\n",
                "for cases of the form `ex: T`",
            ),
            (
                "import language.experimental.saferExceptions\nclass E extends Exception\n\
                 object X { def unapply(e: E): Option[Int] = Some(1) }\n\
                 def f(): Int = try 1 catch { case X(_) => 2 }\n",
                "for cases of the form `ex: T`",
            ),
            (
                "import language.experimental.saferExceptions\nclass E extends Exception\nclass F extends Exception\n\
                 def f(): Int = try 1 catch { case _: E | _: F => 2 }\n",
                "for cases of the form `ex: T`",
            ),
        ],
        &[
            "class A[T]\nclass B extends A[Int]\ndef f = new A[String]\n",
            "class C[F[_], `x`]\n",
            "class L[T] { class P }\nobject O { def f(x: L[Int]#P) = 1 }\n",
            "trait T { def m = 1 }\nobject O extends T { def i = super[T].m }\n",
            "trait T { def m = 1 }\nobject O { inline def i = { class C extends T { def n = super.m }; 1 } }\n",
            "import language.experimental.saferExceptions\nclass E extends Exception\n\
             def f(): Int = try 1 catch { case _: E => 2 }\n",
            "import language.experimental.saferExceptions\nclass E extends Exception\nclass F extends Exception\n\
             def f(): Int = try { if true then throw E() else throw F() } catch { case _: (E | F) => 2 }\n",
            "import language.experimental.saferExceptions\nclass E extends Exception\n\
             def f(): Int = try 1 catch { case a @ (b: E) => 2 }\n",
        ],
    );
}
