//> using scala 3.9.0
package inl

object Lib:
  inline def kind(x: Int): String = x match
    case 0 => "zero"
    case 1 | 2 => "small"
    case n if n < 0 => "negative " + n
    case _ => "big"
  inline def orElse(o: Option[Int]): Int = o match
    case Some(v) => v
    case None => -1
  inline def safeDiv(x: Int): Int =
    try 10 / x
    catch case e: ArithmeticException => -1
  inline def fail(msg: String): Nothing = throw new IllegalStateException(msg)
  inline def local(x: Int): Int =
    val y = x + 1
    def twice(z: Int): Int = z * 2
    twice(y)
  inline def adder(x: Int): Int => Int = (a: Int) => a + x
  inline def pair(x: Int): List[Int] = List(x, x + 1)
  transparent inline def plusOne(x: Int): Int = x + 1
  inline def viaTransparent(x: Int): Int = plusOne(x) * 2
  inline def describe(inline n: Int): String = inline n match
    case 0 => "zero"
    case k if k > 9 => "many"
    case _ => "some"
  inline def sign(inline n: Int): Int = inline if n < 0 then -1 else 1
  inline def typeName[T]: String = inline compiletime.erasedValue[T] match
    case _: Int => "Int"
    case _ => "other"
  inline def firstOr(o: Option[Int]): Int = inline o match
    case Some(v) => v
    case _ => 0
  inline def sepOr(inline min: Int = 0, inline sep: => String = null): String =
    if sep == null then "none " + min else sep
  inline def elemCount[T](using m: deriving.Mirror.Of[T]): Int = inline m match
    case p: deriving.Mirror.ProductOf[T] => size[p.MirroredElemTypes]
    case s: deriving.Mirror.SumOf[T] => size[s.MirroredElemTypes]
  inline def size[T <: Tuple]: Int = inline compiletime.erasedValue[T] match
    case _: EmptyTuple => 0
    case _: (t *: ts) => 1 + size[ts]
  inline def mk(n: Int): Int =
    class Foo(val k: Int):
      def twice = k * 2
    new Foo(n).twice + 1
  inline def anon(s: String): Runnable = new Runnable:
    def run() = println("hi " + s)
  inline def shown: String =
    import Instances.given
    compiletime.summonInline[Show[Int]].show(1)
  inline def shownBounded: String =
    import Instances.given Show[Int]
    compiletime.summonInline[Show[Int]].show(2)
  inline def renamed(n: Int): Int =
    import Instances.twice as double
    double(n)
  inline def excluded(n: Int): Int =
    import Instances.{twice as _, *}
    thrice(n)

trait Show[A]:
  def show(a: A): String

object Instances:
  given Show[Int] = a => "imported " + a
  def twice(n: Int): Int = n * 2
  def thrice(n: Int): Int = n * 3

object Overloads:
  def andThen[A, B](a: A)(f: A => B): B = f(a)
  def andThen[A, B](a: A)(f: PartialFunction[A, B]): Option[B] = f.lift(a)

trait Greeter:
  def greet(s: String): String

object Greeters:
  def hello: Greeter = s => "hello " + s

object Scopes:
  inline def scoped: String =
    val before = compiletime.summonInline[Show[Int]].show(3)
    import Instances.given
    before + " " + compiletime.summonInline[Show[Int]].show(4)
  inline def inClass(n: Int): Int =
    class C:
      import Instances.twice
      export Instances.thrice
      def run = twice(n)
    C().run + C().thrice(n)
