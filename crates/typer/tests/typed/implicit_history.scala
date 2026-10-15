trait Foo { def next: Foo }
object Foo {
  implicit def foo(implicit rec: => Foo): Foo = new Foo { def next = rec }
}

case class E(x: E | Null)
object Outer {
  given e: E(null)
}
import Outer.given

trait Countable[T]
case class Z()
trait Sum[+A, +B]
trait shaped[A, B]

object History {
  def knot = implicitly[Foo]

  implicit lazy val n: Int = 23
  def byName(implicit y: => Int): Int = y
  def plain = byName

  given f(using e: E): E(e)
  def diverging = summon[E]

  implicit def zShape: Z `shaped` Sum[Z, Z] = new (Z `shaped` Sum[Z, Z]) {}
  implicit def shapedCountable[T, U](implicit ev1: shaped[T, U], ev2: Countable[U]): Countable[T] =
    new Countable[T] {}
  implicit def sumCountable[T, U](implicit ev: => Countable[T]): Countable[Sum[T, U]] =
    new Countable[Sum[T, U]] {}
  def shapedKnot = implicitly[Countable[Z]]
}
