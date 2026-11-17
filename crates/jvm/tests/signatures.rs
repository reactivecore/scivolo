//! Generic signatures (JVMS §4.7.9.1) of classes, methods and fields, as dotty writes them.

use std::path::Path;
use syntax::config::Config;
use syntax::source::SourceFile;
use tasty::Universe;

const SOURCE: &str = r#"package sigs

class Meter(val v: Double) extends AnyVal
class Name(val s: String) extends AnyVal
class Box[A](val a: A)
trait HK[F[_]]
trait Variant[+A, -B]
class Bounded[A <: Comparable[A]]
trait Marker

class Sigs[A] {
  type Elem = A
  type U = Unit
  val field: List[A] = Nil
  var count: Int = 0
  def plain(x: Int): String = ""
  def poly[B](b: B): B = b
  def shadow[A](a: A, outer: Sigs[A]): A = a
  def shadowBoth[A, A1](a: A, b: A1, outer: Sigs[A]): A1 = b
  def shadowAlias[A](a: A, e: Elem): A = a
  def shadowInfer[A](a: A) = field
  def shadowInfer2[A, A1](a: A, b: A1) = (field, b)
  def byName(x: => A): A = x
  def byNameInt(x: => Int): Int = x
  def repeated(xs: A*): Int = 0
  def repeatedInt(xs: Int*): Int = 0
  def meter(m: Meter): Meter = m
  def meters(ms: List[Meter]): Option[Name] = None
  def intBound[B <: Int](b: B): B = b
  def unitBound[B <: Unit](b: B): B = b
  def unitAliasBound[B <: U](b: B): B = b
  def unitAlias(x: List[Int]): U = ()
  def meterBound[B <: Meter](b: B): B = b
  def hk[F[_]](f: F[Int]): F[String] = ???
  def hkBound[F[X] <: List[X]](f: F[A]): F[Int] = ???
  def wildcard(x: Box[?]): Box[? <: A] = ???
  def lower(x: Box[? >: String]): Unit = ()
  def boxed(x: List[Int]): Map[Long, Unit] = ???
  def arr(x: Array[A]): Array[Int] = ???
  def arrList(x: Array[List[A]]): Unit = ()
  def fn(f: Int => A): A = ???
  def nullable(x: List[A] | Null): String | Null = null
  def inter[B <: Comparable[B] & java.io.Serializable](b: B): B = b
  def interBounded[B <: Comparable[B]](b: B & java.io.Serializable): Unit = ()
  def chainTrait[A <: Marker, B <: A](x: B & Comparable[B]): Unit = ()
  def chainClass[A <: Box[Int], B <: A, C <: B](x: Comparable[C] & C): Unit = ()
  def chainArray[A <: Marker, B <: A](x: Array[B]): Array[B] = x
  def chainInt[A <: Int, B <: A](x: B & Comparable[B]): B = x
  def chainKind[F[x] <: Seq[x], B <: F[Int]](x: Array[B]): Unit = ()
  def chainIdentity[K[x] <: x, B <: K[Marker]](x: B & java.io.Serializable): Unit = ()
  def identityInt[K[x] <: x](a: K[Int]): K[Int] = a
  def unit(): Unit = ()
  def nothing: Nothing = ???
  def anyRef(x: AnyRef, y: Any): Object = x
}

class IntBound[T <: Int](val t: T) {
  def get: T = t
}

class Wrap[T](val t: List[T]) extends AnyVal {
  def pick[U](u: U): T = t.head
}

trait Tr[X] {
  def get(x: X): List[X] = Nil
}
class Impl extends Tr[String]

class Sub extends Sigs[String] with HK[List] with Variant[Int, Unit]

trait SelfRef {
  def pair[B](b: B): (B, this.type) = (b, this)
}
class Outer[A] {
  class Inner extends SelfRef
}
trait Arr[T] {
  def mk(n: Int): Array[T] = ???
}
class ArrRef[T <: AnyRef] extends Arr[T]

trait Hidden {
  private[sigs] def hidden: Int = 1
}
object Fwd extends Hidden {
  def poly[F[_]](f: [t] => t => F[t]): Int = 0
}
trait Inl {
  inline def apply(n: Int): Int = n
}
object Inl {
  def apply(): String = ""
}
trait AbsInl {
  inline def apply(n: Int): Int
}
class InlImpl extends AbsInl {
  inline def apply(n: Int): Int = n
}
object InlImpl {
  def apply(): String = ""
}
object Conv extends scala.collection.convert.StreamExtensions
object OrdInt extends scala.math.Ordering.IntOrdering
"#;

/// The signatures of forwarders: mixin forwarders seen from their class and static forwarders.
const FORWARDERS: &[(&str, &str, Option<&str>)] = &[
    ("Outer$Inner", "pair", Some("<B:Ljava/lang/Object;>(TB;)Lscala/Tuple2<TB;Lsigs/Outer<TA;>.Inner;>;")),
    ("ArrRef", "mk", None),
    ("Fwd", "hidden", None),
    ("Fwd", "poly", Some("<F:Ljava/lang/Object;>(Lscala/PolyFunction;)I")),
    ("Inl", "apply", None),
    ("InlImpl", "apply", None),
    ("OrdInt$", "max", None),
    ("OrdInt", "max", None),
    (
        "Conv",
        "MapHasParKeyValueStream",
        Some(
            "<K:Ljava/lang/Object;V:Ljava/lang/Object;\
             CC::Lscala/collection/MapOps<Ljava/lang/Object;Ljava/lang/Object;Lscala/collection/Map;*>;>\
             (Lscala/collection/MapOps<TK;TV;Lscala/collection/Map;*>;)\
             Lscala/collection/convert/StreamExtensions$MapHasParKeyValueStream<TK;TV;TCC;>;",
        ),
    ),
];

/// The method signatures of `SOURCE` that dotty 3.9.0 writes: class, method, signature; but for
/// upstream bugs 20 and 21 of `docs/conformance.md`.
const METHODS: &[(&str, &str, Option<&str>)] = &[
    ("Sigs", "field", Some("()Lscala/collection/immutable/List<TA;>;")),
    ("Sigs", "count", None),
    ("Sigs", "plain", None),
    ("Sigs", "poly", Some("<B:Ljava/lang/Object;>(TB;)TB;")),
    ("Sigs", "shadow", Some("<A:Ljava/lang/Object;>(TA;Lsigs/Sigs<TA;>;)TA;")),
    ("Sigs", "shadowBoth", Some("<A:Ljava/lang/Object;A1:Ljava/lang/Object;>(TA;TA1;Lsigs/Sigs<TA;>;)TA1;")),
    ("Sigs", "shadowAlias", Some("<A1:Ljava/lang/Object;>(TA1;TA;)TA1;")),
    ("Sigs", "shadowInfer", Some("<A1:Ljava/lang/Object;>(TA1;)Lscala/collection/immutable/List<TA;>;")),
    (
        "Sigs",
        "shadowInfer2",
        Some(
            "<A1:Ljava/lang/Object;A11:Ljava/lang/Object;>(TA1;TA11;)\
             Lscala/Tuple2<Lscala/collection/immutable/List<TA;>;TA11;>;",
        ),
    ),
    ("Sigs", "byName", Some("(Lscala/Function0<TA;>;)TA;")),
    ("Sigs", "byNameInt", Some("(Lscala/Function0<Ljava/lang/Object;>;)I")),
    ("Sigs", "repeated", Some("(Lscala/collection/immutable/Seq<TA;>;)I")),
    ("Sigs", "repeatedInt", Some("(Lscala/collection/immutable/Seq<Ljava/lang/Object;>;)I")),
    ("Sigs", "meter", None),
    ("Sigs", "meters", Some("(Lscala/collection/immutable/List<Lsigs/Meter;>;)Lscala/Option<Lsigs/Name;>;")),
    ("Sigs", "intBound", Some("<B:Ljava/lang/Object;>(I)I")),
    ("Sigs", "unitBound", Some("<B:Lscala/runtime/BoxedUnit;>(TB;)TB;")),
    ("Sigs", "unitAliasBound", Some("<B:Lscala/runtime/BoxedUnit;>(TB;)TB;")),
    ("Sigs", "unitAlias", Some("(Lscala/collection/immutable/List<Ljava/lang/Object;>;)V")),
    ("Sigs", "meterBound", Some("<B:Lsigs/Meter;>(D)D")),
    ("Sigs", "hk", Some("<F:Ljava/lang/Object;>(Ljava/lang/Object;)Ljava/lang/Object;")),
    (
        "Sigs",
        "hkBound",
        Some(
            "<F:Lscala/collection/immutable/List<Ljava/lang/Object;>;>(Lscala/collection/immutable/List<TA;>;)\
             Lscala/collection/immutable/List<Ljava/lang/Object;>;",
        ),
    ),
    ("Sigs", "wildcard", Some("(Lsigs/Box<*>;)Lsigs/Box<+TA;>;")),
    ("Sigs", "lower", Some("(Lsigs/Box<-Ljava/lang/String;>;)V")),
    (
        "Sigs",
        "boxed",
        Some(
            "(Lscala/collection/immutable/List<Ljava/lang/Object;>;)\
             Lscala/collection/immutable/Map<Ljava/lang/Object;Lscala/runtime/BoxedUnit;>;",
        ),
    ),
    ("Sigs", "arr", None),
    ("Sigs", "arrList", Some("([Lscala/collection/immutable/List<TA;>;)V")),
    ("Sigs", "fn", Some("(Lscala/Function1<Ljava/lang/Object;TA;>;)TA;")),
    ("Sigs", "nullable", Some("(Lscala/collection/immutable/List<TA;>;)Ljava/lang/String;")),
    ("Sigs", "inter", Some("<B::Ljava/io/Serializable;:Ljava/lang/Comparable<TB;>;>(TB;)TB;")),
    ("Sigs", "interBounded", Some("<B::Ljava/lang/Comparable<TB;>;>(Ljava/io/Serializable;)V")),
    ("Sigs", "chainTrait", Some("<A::Lsigs/Marker;B::TA;>(Ljava/lang/Comparable<TB;>;)V")),
    ("Sigs", "chainClass", Some("<A:Lsigs/Box<Ljava/lang/Object;>;B:TA;C:TB;>(TC;)V")),
    ("Sigs", "chainArray", Some("<A::Lsigs/Marker;B::TA;>([TB;)[TB;")),
    ("Sigs", "chainInt", Some("<A:Ljava/lang/Object;B:Ljava/lang/Object;>(I)I")),
    (
        "Sigs",
        "chainKind",
        Some(
            "<F::Lscala/collection/immutable/Seq<Ljava/lang/Object;>;B::Lscala/collection/immutable/Seq<Ljava/lang/Object;>;>([TB;)V",
        ),
    ),
    ("Sigs", "chainIdentity", Some("<K:Ljava/lang/Object;B::Lsigs/Marker;>(Ljava/io/Serializable;)V")),
    ("Sigs", "identityInt", Some("<K:Ljava/lang/Object;>(I)I")),
    ("Sigs", "unit", None),
    ("Sigs", "nothing", None),
    ("Sigs", "anyRef", None),
    ("Box", "<init>", Some("(TA;)V")),
    ("Box", "a", Some("()TA;")),
    ("Wrap", "<init>", Some("(Lscala/collection/immutable/List<TT;>;)V")),
    ("Wrap", "pick", Some("<U:Ljava/lang/Object;>(TU;)TT;")),
    (
        "Wrap",
        "pick$extension",
        Some("<U:Ljava/lang/Object;T:Ljava/lang/Object;>(Lscala/collection/immutable/List<TT;>;TU;)TT;"),
    ),
    (
        "Wrap$",
        "pick$extension",
        Some("<U:Ljava/lang/Object;T:Ljava/lang/Object;>(Lscala/collection/immutable/List<TT;>;TU;)TT;"),
    ),
    ("Wrap$", "hashCode$extension", Some("<T:Ljava/lang/Object;>(Lscala/collection/immutable/List<TT;>;)I")),
    ("Meter", "v", None),
    ("IntBound", "<init>", None),
    ("IntBound", "get", None),
    ("Tr", "get", Some("(TX;)Lscala/collection/immutable/List<TX;>;")),
    ("Tr", "get$", None),
    ("Impl", "get", None),
];

/// The method descriptors of `SOURCE` that dotty writes, where erasure follows a chain of bounds.
const DESCRIPTORS: &[(&str, &str, &str)] = &[
    ("Sigs", "chainTrait", "(Ljava/lang/Comparable;)V"),
    ("Sigs", "chainClass", "(Lsigs/Box;)V"),
    ("Sigs", "chainArray", "([Lsigs/Marker;)[Lsigs/Marker;"),
    ("Sigs", "chainInt", "(I)I"),
    ("Sigs", "chainKind", "([Lscala/collection/immutable/Seq;)V"),
    ("Sigs", "chainIdentity", "(Ljava/io/Serializable;)V"),
    ("Sigs", "identityInt", "(I)I"),
];

/// The field signatures of `SOURCE` that dotty writes.
const FIELDS: &[(&str, &str, Option<&str>)] = &[
    ("Sigs", "field", Some("Lscala/collection/immutable/List<TA;>;")),
    ("Sigs", "count", None),
    ("Box", "a", Some("TA;")),
    ("Wrap", "t", Some("Lscala/collection/immutable/List<TT;>;")),
    ("Meter", "v", None),
    ("IntBound", "t", None),
];

/// The class signatures of `SOURCE` that dotty writes.
const CLASSES: &[(&str, Option<&str>)] = &[
    ("Sigs", Some("<A:Ljava/lang/Object;>Ljava/lang/Object;")),
    (
        "Sub",
        Some(
            "Lsigs/Sigs<Ljava/lang/String;>;Lsigs/HK<Lscala/collection/immutable/List;>;\
             Lsigs/Variant<Ljava/lang/Object;Lscala/runtime/BoxedUnit;>;",
        ),
    ),
    ("Variant", Some("<A:Ljava/lang/Object;B:Ljava/lang/Object;>Ljava/lang/Object;")),
    ("Bounded", Some("<A::Ljava/lang/Comparable<TA;>;>Ljava/lang/Object;")),
    ("HK", Some("<F:Ljava/lang/Object;>Ljava/lang/Object;")),
    ("Box", Some("<A:Ljava/lang/Object;>Ljava/lang/Object;")),
    ("Wrap", Some("<T:Ljava/lang/Object;>Ljava/lang/Object;")),
    ("Wrap$", Some("Ljava/lang/Object;")),
    ("Meter", Some("Ljava/lang/Object;")),
    ("IntBound", Some("<T:Ljava/lang/Object;>Ljava/lang/Object;")),
    ("Impl", Some("Ljava/lang/Object;Lsigs/Tr<Ljava/lang/String;>;")),
];

/// The classes of `SOURCE` compiled for the JVM; none without the library jars.
fn classes() -> Option<Vec<classfile::ClassInfo>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    let mut u = Universe::open(&jars).expect("the library");
    let sources = [SourceFile::new("sigs.scala", SOURCE)];
    let backend = (lower::Backend::Jvm, jvmgen::unit);
    let compiled = lower::compile_units(&mut u, &sources, Config::from_options(&[]), None, backend);
    let (classes, _) = compiled.unwrap_or_else(|r| panic!("{}", r.what));
    Some(classes.iter().map(|c| classfile::read(&c.bytes).expect("a class file")).collect())
}

#[test]
fn generic_signatures_as_dotty_writes_them() {
    let Some(classes) = classes() else { return };
    let class = |name: &str| {
        let internal = format!("sigs/{name}");
        classes.iter().find(|c| c.name == internal).unwrap_or_else(|| panic!("no class {internal}"))
    };
    for &(c, sig) in CLASSES {
        assert_eq!(class(c).signature.as_deref(), sig, "class {c}");
    }
    let method = |c: &str, m: &str| {
        class(c).methods.iter().find(|x| x.name == m).unwrap_or_else(|| panic!("no method {c}.{m}"))
    };
    for &(c, m, sig) in METHODS.iter().chain(FORWARDERS) {
        assert_eq!(method(c, m).signature.as_deref(), sig, "method {c}.{m}");
    }
    for &(c, m, descriptor) in DESCRIPTORS {
        assert_eq!(method(c, m).descriptor, descriptor, "method {c}.{m}");
    }
    for &(c, f, sig) in FIELDS {
        let field =
            class(c).fields.iter().find(|x| x.name == f).unwrap_or_else(|| panic!("no field {c}.{f}"));
        assert_eq!(field.signature.as_deref(), sig, "field {c}.{f}");
    }
}
