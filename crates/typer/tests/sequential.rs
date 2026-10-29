//! Units typed one after another in one universe, each seeing the objects of the ones before.

use std::path::{Path, PathBuf};
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::names::Names;
use syntax::parse::Parser;
use syntax::source::SourceFile;
use tasty::{Kind, Universe};
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

fn type_unit(u: &mut Universe, units: &mut Units, name: &str, text: &str) -> Result<(), String> {
    let source = SourceFile::new(name, text);
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::from_options(&[]));
    let root = p.parse();
    let cfg = p.s.cfg;
    let root = Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, &source, cfg).unit(root);
    assert!(!p.s.diags.has_errors(), "syntax errors in {name}");
    match units.type_unit(u, &mut p.ast, (p.s.names, &mut p.fresh), (root, cfg), None) {
        Ok(_) => Ok(()),
        Err(Fallback::Unsupported { what, .. }) => Err(format!("unsupported {what}")),
        Err(Fallback::Errors(d)) => {
            let messages: Vec<&str> = d.unique().iter().map(|d| d.message.as_str()).collect();
            Err(format!("type errors {messages:?}"))
        }
    }
}

/// The type of member `member` of top-level object `object`.
fn member_type(u: &mut Universe, object: &str, member: &str) -> String {
    let root = u.root();
    let class = u.decl(root, &format!("{object}$")).expect("a lookup");
    let class = *class.iter().find(|&&c| u.symbol(c).kind == Kind::Class).expect("the module class");
    let found = u.decl(class, member).expect("a lookup");
    let t = u.info(found[0]);
    u.show(t)
}

/// Types `lines` in order as units `rs$line$N`, each importing the ones before.
fn type_lines(u: &mut Universe, lines: &[&str]) {
    let mut units = Units::default();
    for (i, line) in lines.iter().enumerate() {
        let imports: String = (1..=i).map(|n| format!("import rs$line${n}.{{given, *}}; ")).collect();
        let text = format!("object rs$line${} {{ {imports}{line} }}", i + 1);
        assert_eq!(type_unit(u, &mut units, "line.scala", &text), Ok(()), "{text}");
    }
}

#[test]
fn second_unit_imports_the_first() {
    let Some(mut u) = universe() else { return };
    type_lines(
        &mut u,
        &["val x = 1; def f(y: Int): Int = y + x; class C(val a: Int)", "val y = f(x) + new C(2).a"],
    );
    assert_eq!(member_type(&mut u, "rs$line$2", "y"), "scala.Int");
}

#[test]
fn long_argument_lists_of_constants() {
    let Some(mut u) = universe() else { return };
    let ints: Vec<String> = (0..1000).map(|i| i.to_string()).collect();
    let strings: Vec<String> = (0..1000).map(|i| format!("\"s{i}\"")).collect();
    let line = format!("val xs = List({}); val ys = Seq({})", ints.join(", "), strings.join(", "));
    type_lines(&mut u, &[&line]);
    assert_eq!(member_type(&mut u, "rs$line$1", "xs"), "scala.collection.immutable.List[Int]");
    assert_eq!(member_type(&mut u, "rs$line$1", "ys"), "scala.collection.immutable.Seq[String]");
}

#[test]
fn classes_of_earlier_units() {
    let Some(mut u) = universe() else { return };
    type_lines(
        &mut u,
        &[
            "sealed trait S; case class P(x: Int) extends S; class C(val a: Int)",
            "case class Q(y: Int) extends S; def h(s: S): Int = s match { case P(n) => n; case _ => 0 }",
            "val y = h(P(1)) + P(2).x + C(3).a + h(Q(4))",
        ],
    );
    assert_eq!(member_type(&mut u, "rs$line$3", "y"), "scala.Int");
}

#[test]
fn unit_after_a_failed_one() {
    let Some(mut u) = universe() else { return };
    let mut units = Units::default();
    let first = "object rs$line$1 { def f(xs: Int*): Int = xs.sum }";
    assert_eq!(type_unit(&mut u, &mut units, "line.scala", first), Ok(()));
    let failed = "object rs$line$2 { import rs$line$1.*; val s: String = f(1) }";
    assert!(type_unit(&mut u, &mut units, "line.scala", failed).is_err());
    let third = "object rs$line$3 { import rs$line$1.*; val y = f(1, 2) + f() }";
    assert_eq!(type_unit(&mut u, &mut units, "line.scala", third), Ok(()));
    assert_eq!(member_type(&mut u, "rs$line$3", "y"), "scala.Int");
}

#[test]
fn definition_left_incomplete_by_a_failed_unit() {
    let Some(mut u) = universe() else { return };
    let mut units = Units::default();
    let first = "object rs$line$1 { case class T(f: Int ?=> Int) { def g = 1 } }";
    assert_eq!(
        type_unit(&mut u, &mut units, "line.scala", first),
        Err("unsupported case class element of a context function type".into())
    );
    let second = "object rs$line$2 { import rs$line$1.*; def h(t: T) = t.g }";
    assert_eq!(
        type_unit(&mut u, &mut units, "line.scala", second),
        Err("unsupported definition left incomplete by a failed unit".into())
    );
}

#[test]
fn redefined_classes_and_forgotten_lines() {
    let Some(mut u) = universe() else { return };
    let mut units = Units::default();
    let lines = [
        "object rs$line$1 { class C { def f: Int = 1 }; val c = new C }",
        "object rs$line$2 { class C { def f: String = \"\" } }",
        "object rs$line$3 { import rs$line$1.c; import rs$line$2.C; val x = c.f + new C().f.length }",
    ];
    for line in lines {
        assert_eq!(type_unit(&mut u, &mut units, "line.scala", line), Ok(()), "{line}");
    }
    assert_eq!(member_type(&mut u, "rs$line$3", "x"), "scala.Int");
    units.forget(&mut u, "rs$line$2");
    let again = [
        "object rs$line$2 { class C { def f: Boolean = true } }",
        "object rs$line$4 { import rs$line$2.C; val y = new C().f }",
    ];
    for line in again {
        assert_eq!(type_unit(&mut u, &mut units, "line.scala", line), Ok(()), "{line}");
    }
    assert_eq!(member_type(&mut u, "rs$line$4", "y"), "scala.Boolean");
}

#[test]
fn aliases_completed_after_use() {
    let Some(mut u) = universe() else { return };
    let cases = [
        (
            "object A1 { class Box[T](t: T) { def get: T = t }; def g = b.get + 1; val b: B = new Box(1); type B = Box[Int] }",
            "A1",
            "g",
        ),
        ("object A2 { def f(p: P[Int]) = p._1 + 1; def g = f((1, 2)); type P[X] = (X, X) }", "A2", "g"),
        (
            "object A3 { def g = c.get + c.get; val c = new C; class C extends Base[E]; type E = Int; class Base[T] { def get: T = ??? } }",
            "A3",
            "g",
        ),
    ];
    for (text, object, member) in cases {
        assert_eq!(type_unit(&mut u, &mut Units::default(), "a.scala", text), Ok(()), "{text}");
        assert_eq!(member_type(&mut u, object, member), "scala.Int", "{text}");
    }
}

#[test]
fn deferred_members_of_earlier_units() {
    let Some(mut u) = universe() else { return };
    type_lines(&mut u, &["trait T { def a: Int; def b: Int = 1 }", "class C extends T { def a = 2 }"]);
    let root = u.root();
    let object = u.decl(root, "rs$line$1$").expect("a lookup");
    let object = *object.iter().find(|&&c| u.symbol(c).kind == Kind::Class).expect("the module class");
    let t = u.decl(object, "T").expect("a lookup")[0];
    let a = u.decl(t, "a").expect("a lookup")[0];
    let b = u.decl(t, "b").expect("a lookup")[0];
    assert!(u.is_deferred(a));
    assert!(!u.is_deferred(b));
    let line2 = u.decl(root, "rs$line$2$").expect("a lookup");
    let line2 = *line2.iter().find(|&&c| u.symbol(c).kind == Kind::Class).expect("the module class");
    let c = u.decl(line2, "C").expect("a lookup")[0];
    let c_a = u.decl(c, "a").expect("a lookup")[0];
    assert!(!u.is_deferred(c_a));
}

#[test]
fn transparent_inline_refusals() {
    let Some(mut u) = universe() else { return };
    let mut units = Units::default();
    let refusals = [
        ("object A { val v = a(1); transparent inline def a(x: Int): Any = v }", "cyclic reference"),
        (
            "object B { transparent inline def unapply(x: Int): Option[Int] = Some(x) }
             object C { def f(x: Int) = x match { case B(y) => y } }",
            "transparent inline extractor",
        ),
        (
            "object D { abstract class K { type T; val t: T }; class L extends K { type T = Int; val t = 1 }
             transparent inline def d(k: K, n: Int): k.T = k.t
             val r = d(n = 1, k = new L) }",
            "dependent result type",
        ),
        (
            "object E { class K { type T = Int; val t: T = 1 }; transparent inline def d(k: K, n: Int): k.T = k.t
             val r = d(n = 1, k = new K) }",
            "",
        ),
        ("object rs$line$1 { transparent inline def e: Any = 1 }", ""),
        ("object rs$line$2 { import rs$line$1.*; val x = e }", "transparent inline method call"),
    ];
    for (text, what) in refusals {
        let expected = if what.is_empty() { Ok(()) } else { Err(format!("unsupported {what}")) };
        assert_eq!(type_unit(&mut u, &mut units, "t.scala", text), expected, "{text}");
    }
}

#[test]
fn match_types_reduce_in_conformance() {
    let Some(mut u) = universe() else { return };
    let mut units = Units::default();
    let cases = [
        (
            "object M { type E[X] = X match { case String => Char; case Array[t] => t }
             def c[X](x: X): E[X] = ???; val s: Char = c(\"a\"); val i: Int = c(Array(1)) }",
            "",
        ),
        ("object N { val h: Int = (1, \"a\").head; val n: 2 = (1, \"a\").size }", ""),
        ("object O { val d: Tuple.Head[EmptyTuple] = ??? }", "match type that may reduce"),
        (
            "object S { class C[-T]; type G[X] = X match { case C[List[t]] => t } }",
            "match type pattern outside the subset",
        ),
        (
            "object P { type M[X] = X match { case Array[t] => t => Unit }; def f[X](m: M[X]): Any => Unit = m }",
            "adaptation",
        ),
        (
            "object Q { type M[X] <: Int = X match { case Int => String } }",
            "match type case body outside the bound",
        ),
        ("object R { val b: scala.compiletime.ops.any.==[1, 1L] = false }", "adaptation"),
    ];
    for (text, what) in cases {
        let expected = if what.is_empty() { Ok(()) } else { Err(format!("unsupported {what}")) };
        assert_eq!(type_unit(&mut u, &mut units, "t.scala", text), expected, "{text}");
    }
    let text = "object T { type M[X] <: F = X match { case Int => String }; type F = Undefined }";
    let typed = type_unit(&mut u, &mut units, "t.scala", text);
    assert_eq!(typed, Err("type errors [\"Not found: type Undefined\"]".into()));
}

#[test]
fn implicit_results_referring_to_their_parameters_leave_open_types() {
    let Some(mut u) = universe() else { return };
    let text = "trait Fo { type T }; object Fo { implicit val f: Fo { type T = Int } = new Fo { type T = Int } }
        object D { def g(implicit x: Fo): x.T = ???; def h[A](o: Option[A]): Option[A] = o; val o = h(Some(g)) }";
    assert_eq!(type_unit(&mut u, &mut Units::default(), "d.scala", text), Ok(()));
    assert_eq!(member_type(&mut u, "D", "o"), "scala.Option[scala.Int]");
}

/// Whether typing `text` as a unit after `before` types it or leaves the unresolved `c` to the
/// fallback.
fn finds_toplevel(before: &[&str], text: &str) -> bool {
    let Some(mut u) = universe() else { return true };
    let mut units = Units::default();
    for (i, b) in before.iter().enumerate() {
        assert_eq!(type_unit(&mut u, &mut units, &format!("before{i}.scala"), b), Ok(()), "{b}");
    }
    let outcome = type_unit(&mut u, &mut units, "main.scala", text);
    matches!(&outcome, Ok(())) || matches!(&outcome, Err(e) if e.as_str() == "unsupported unresolved name c")
}

#[test]
fn toplevel_definitions_used_in_an_object() {
    let main = "object Main:\n  def main(args: Array[String]) =\n    val y = c\n";
    assert!(finds_toplevel(&[], &format!("val c = 1\n\n{main}")));
    assert!(finds_toplevel(&[], &format!("{main}\nval c = 1\n")));
    assert!(finds_toplevel(&[], &format!("{main}\ndef c = 1\n")));
    assert!(finds_toplevel(&["val c = 1\n"], main));
}

/// The outcome of typing `text` with errors taking precedence over unsupported constructs.
fn errors_first(text: &str) -> Result<(), String> {
    let Some(mut u) = universe() else { return Ok(()) };
    type_unit(&mut u, &mut Units::errors_first(), "main.scala", text)
}

#[test]
fn no_false_errors_behind_unsupported_constructs() {
    let Some(_) = universe() else { return };
    let cases = [
        (
            "class C:\n  class D\nclass Baz:\n  val c = new C\n  export c.*\nobject M:\n  val d = Baz().D()\n",
            "export of a class without a companion object",
        ),
        ("trait T\nobject S extends T { self =>\n  def f() = self.hashCode\n}\n", "named self of an object"),
        (
            "trait B\ntrait A { self: B => }\nclass C extends A\n",
            "class not conforming to a parent's self type",
        ),
        (
            "trait B\ntrait A { self: B => }\nobject M:\n  val a = new A {}\n",
            "class not conforming to a parent's self type",
        ),
        (
            "trait B\ntrait A { self: B => }\ntrait A2 extends A\nobject O extends A2 with B\n",
            "class not conforming to a parent's self type",
        ),
        (
            "trait B\nclass A { self: B => }\nobject M:\n  val a = new A()\n",
            "new of a class not conforming to its self type",
        ),
        (
            "trait A[T] { self: T => }\nclass C extends A[String]\n",
            "class not conforming to a parent's self type",
        ),
        (
            "object M:\n  type F[X]\n  def f(x: Any) = x match\n    case y: F[Int] => y\n",
            "type pattern needing a type test or type binding",
        ),
        (
            "import scala.language.experimental.genericNumberLiterals\nobject M:\n  val x: BigInt = 13232202002020202020202\n",
            "generic number literal",
        ),
        (
            "class A { val x = 1 }\nclass B extends A { override lazy val x = 2 }\n",
            "lazy value overriding a non-lazy value",
        ),
        (
            "trait A { lazy val x = 1 }\nclass B extends A { override val x = 2 }\n",
            "non-lazy value overriding a lazy value",
        ),
        (
            "class A(val x: Int)\nclass B extends A(1) { override def x = 2 }\n",
            "unstable member overriding a stable value",
        ),
        (
            "class A { final def x = 1 }\nclass B extends A { override def x = 2 }\n",
            "override of a final member",
        ),
        (
            "class A { var x = 1 }\nclass B extends A { override def x = 2 }\n",
            "override of a mutable variable",
        ),
        (
            "abstract class A { def f: Int }\nclass C extends A\n",
            "concrete class with an unimplemented member",
        ),
        (
            "class A { def f = 1 }\nabstract class B extends A { def f: Int }\nclass C extends B\n",
            "concrete class with an unimplemented member",
        ),
        (
            "trait A { def f(x: Int): Int }\nclass C extends A { def f(x: String) = 1 }\n",
            "concrete class with an unimplemented member",
        ),
        (
            "trait A { def a$$super$f: Int }\nclass C extends A\n",
            "concrete class with an unimplemented member",
        ),
        (
            "class G:\n  val ensuring = 1\nobject M:\n  val h = G().ensuring(true)\n",
            "application of a non-method",
        ),
        (
            "class C:\n  val n = 1\nobject C:\n  extension (c: C) def n(i: Int) = i\nobject M:\n  val x = (new C).n(1)\n",
            "application of a non-method",
        ),
    ];
    for (text, what) in cases {
        assert_eq!(errors_first(text), Err(format!("unsupported {what}")), "{text}");
    }
    assert_eq!(errors_first("object M:\n  val x = y\n"), Err("type errors [\"Not found: y\"]".into()));
    let many = "class Term[T]\nobject M:\n  val t: Term[Int, Kind] = ???\n";
    let too_many = "Too many type arguments for Term[T]\\nexpected: [T]\\nactual:   [Int, Kind]";
    assert_eq!(errors_first(many), Err(format!("type errors [\"{too_many}\"]")));
    let few = "class Term[A, B]\nobject M:\n  val t: Term[Kind] = ???\n";
    let not_enough = "Not enough type arguments for Term[A, B]\\nexpected: [A, B]\\nactual:   [Kind]";
    assert_eq!(errors_first(few), Err(format!("type errors [\"{not_enough}\"]")));
    let kept = "class P\nobject M:\n  def p(x: P[Int], l: List[Int, P]): Int = x\n";
    let errors = [
        "P does not take type parameters",
        "Too many type arguments for List[A]\nexpected: [A]\nactual:   [Int, P]",
        "Found:    (x : P)\nRequired: Int",
    ];
    assert_eq!(errors_first(kept), Err(format!("type errors {errors:?}")));
    let untyped = "import scala.language.experimental.genericNumberLiterals\nobject M:\n  val x = 1\n";
    assert_eq!(errors_first(untyped), Ok(()));
    let strict = "import scala.language.strictEquality\nclass A\nclass B\nobject M:\n  val b = A() == B()\n";
    assert_eq!(errors_first(strict), Err("unsupported comparison of unrelated types".into()));
    let unimported =
        strict.replace("object M:\n", "object M:\n  import scala.language.strictEquality as _\n");
    assert_eq!(errors_first(&unimported), Ok(()));
}

#[test]
fn unit_members_replace_classpath_members() {
    let Some(mut u) = universe() else { return };
    let text = "package scala.scalanative.libc\nobject string { def f = 1 }\nobject M { val y = string.f }\n";
    assert_eq!(type_unit(&mut u, &mut Units::default(), "string.scala", text), Ok(()));
    let libc = u.package("scala.scalanative.libc").expect("libc");
    let strings = u.decl(libc, "string").expect("a lookup");
    assert_eq!(strings.len(), 1);
    assert!(u.symbol(strings[0]).tree.is_none());
}

#[test]
fn each_annotation_keeps_its_string() {
    let Some(mut u) = universe() else { return };
    let text =
        "class Ann(s: String) extends scala.annotation.StaticAnnotation\n@Ann(\"a\") @Ann(\"b\") object La\n";
    assert_eq!(type_unit(&mut u, &mut Units::default(), "La.scala", text), Ok(()));
    let root = u.package("").expect("the empty package");
    let la = u.decl(root, "La").expect("a lookup")[0];
    let ann = u.decl(root, "Ann").expect("a lookup")[0];
    assert_eq!(u.annotation_strings(la, ann), [Some("a".to_owned()), Some("b".to_owned())]);
}

#[test]
fn unit_objects_in_jdk_packages_are_values() {
    let Some(mut u) = universe() else { return };
    let text = "package java.io\nobject FilenameFilter { val self: Any = FilenameFilter }\n";
    assert_eq!(type_unit(&mut u, &mut Units::default(), "FilenameFilter.scala", text), Ok(()));
}

#[test]
fn javalib_objects_are_values_when_compiling_javalib() {
    let Some(mut u) = universe() else { return };
    let text = |n: &str| format!("package java.io\nobject {n} {{ val filter: Any = FilenameFilter }}\n");
    let refused = Err("unsupported object of a Java class used as a value".to_owned());
    assert_eq!(type_unit(&mut u, &mut Units::default(), "U1.scala", &text("U1")), refused);
    u.set_javalib_sources();
    assert_eq!(type_unit(&mut u, &mut Units::default(), "U2.scala", &text("U2")), Ok(()));
}

#[test]
fn synthetic_symbols_are_shared() {
    let Some(mut u) = universe() else { return };
    let line = "def f(xs: Int*): Int = if xs.isEmpty then throw Exception() else xs.sum; val y = f(1, 2)";
    for n in 1..=2 {
        let text = format!("object rs$line${n} {{ {line} }}");
        assert_eq!(type_unit(&mut u, &mut Units::default(), "line.scala", &text), Ok(()));
    }
    let scala = u.package("scala").expect("scala");
    let repeated = u.symbol(scala).decls.iter().filter(|&&d| u.symbol(d).name == "<repeated>").count();
    assert_eq!(repeated, 1);
    let ops = u.package("<special-ops>").expect("<special-ops>");
    assert_eq!(u.symbol(ops).decls.len(), 1);
}

#[test]
fn constructs_diverging_from_dotty_are_refused() {
    let Some(mut u) = universe() else { return };
    let cases = [
        (
            "object A { type T[X] = X | Any; def p2[X](using T[X]): X = ???; def p1[X](using Int): X = p2 }",
            "implicit search for a union with type variables",
        ),
        ("object W { val f: ? => Int = x => 1 }", "parameter type inferred from the target of a wildcard"),
        ("object U { val f: (? >: (Int, Int)) => Int = (a, b) => a }", "untupled parameters of a wildcard formal"),
        (
            "class CS extends CharSequence { def length = 0; def charAt(i: Int) = 'a'
             def subSequence(a: Int, b: Int): CharSequence = this; def s = toString }",
            "Object member a JDK interface redeclares",
        ),
        (
            "package lx { class X { class Foo; implicit val foo: Foo = new Foo } }
             package ly { package object y extends lx.X }
             object Ko { import ly.y.{Foo, foo}; implicitly[Foo] }",
            "implicit a package object inherits, through its package",
        ),
        ("object Vg { def g[L <: 1](implicit v: ValueOf[L]) = v.value; val a = g }", "no implicit argument"),
        (
            "object Sg { val a = 1 }
             object Vs { def s[L <: Sg.type](implicit v: ValueOf[L]): L = v.value; val b = s }",
            "no implicit argument",
        ),
        ("object Vu { def u[L <: Unit](implicit v: ValueOf[L]): L = v.value; val c = u }", "no implicit argument"),
        (
            "trait En[A]; object En { implicit def lit[L <: Int](implicit v: ValueOf[L]): En[L] = new En[L] {} }
             object Ve { def f[A <: 1](implicit e: En[A]): Int = 1; val x = f }",
            "no implicit argument",
        ),
        (
            "class A(x: Int) { def this(s: String) = this(); def this() = this(0) }",
            "secondary constructor calling a later constructor",
        ),
        (
            "trait Tag[T]; object Tag { implicit val ti: Tag[Int] = new Tag[Int] {}; implicit val tl: Tag[Long] = new Tag[Long] {} }
             object Cr8 { def mx[T](x: T)(implicit t: Tag[T]): T = x; val o: Option[Long] = Some(mx(3)) }",
            "adaptation",
        ),
        (
            "class O(x: Int) { def this(s: String) = this(this.hashCode) }",
            "this in the arguments of a self-constructor call",
        ),
        ("object N { @native def f(): Int = 1 }", "annotation @scala.native"),
        ("object Lv { def f() = { var x: Int = _; x } }", "`_` other than as a variable field's value"),
        (
            "class Fi(i: java.io.InputStream) extends java.io.FilterInputStream(i) { def d(): Unit = in = null }",
            "assignment to a non-variable",
        ),
        (
            "object Sx { def x = 1; def x_=(v: Int) = () }\nobject Sy { import Sx.x; def f() = { def x_=(v: Int) = (); x = 2 } }",
            "setter of a member shadowed by another `_=`",
        ),
    ];
    for (text, what) in cases {
        let outcome = type_unit(&mut u, &mut Units::default(), "t.scala", text);
        assert_eq!(outcome, Err(format!("unsupported {what}")), "{text}");
    }
}
