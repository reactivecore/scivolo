import scala.reflect.ClassTag

class Foo(val n: Int)

object Test {
  def sibling = { { val it = 1; println(it) }; { val it = 2; println(it) } }

  def inner(using f: Foo): Int = f.n
  def outer(using Foo): Int = inner + 1
  def implicitParam(implicit f: Foo): Int = inner

  def less[T: Ordering](x: T, y: T): Boolean = summon[Ordering[T]].lt(x, y)
  class Box[T: ClassTag](val x: T) { def tag = summon[ClassTag[T]] }
  def newArray[T: ClassTag] = new Array[T](10)
  def newArrayTyped[T: ClassTag]: Array[T] = new Array(10)

  def f[A](x: A) = 1
  def f(x: Int) = 2
  val poly = f("a") + f(1)

  def implicitlyApplied(using Foo) = inner == outer

  def unitDefault(x: Unit = ()) = x
  def byNameDefault(x: => Unit = ()) = x
  val bytes: Array[Byte] = Array(0, 1, 2)

  def switched(x: Int) = (x: @scala.annotation.switch) match { case 1 => "a"; case _ => "b" }
  def unchecked(o: Option[Int]) = (o: @unchecked) match { case Some(i) => i }
}

class Defaults[E](val x: Unit = (), y: Unit)

trait Numbers { def zero: Int; def succ(n: Int): Int; given ord: Ordering[Int] = Ordering.Int }
object Paths {
  def use(numbers: Numbers) = { import numbers.*; succ(zero) }
  def givens(numbers: Numbers) = { import numbers.given; summon[Ordering[Int]] }
}
