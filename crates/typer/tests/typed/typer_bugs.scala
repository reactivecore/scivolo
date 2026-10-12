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

object ByResult {
  def g(x: Int)(y: Int): Int = x + y
  def g(x: Any): Int => Int = (y: Int) => y * 100
  def k(x: Int): Int = 1
  def k(x: Any): String = "any"
  val h: Int => Int = g(1)
  val s: String = k(1)
  val n: Long = k(1)
}

class WithOrdering(val x: Int)(implicit val o: Ordering[Int]) {
  def this() = this(0)
}

object Migration {
  def h(x: Int)(implicit o: Ordering[Int]) = x
  val a = h(2)(Ordering.Int)
  val b = h(2) { Ordering.Int }
}

class Secondary(x: Int, y: String) {
  def this(x: Int) = this(x, "d")
  def this() = { this(0); println("init") }
}

trait Numbers { def zero: Int; def succ(n: Int): Int; given ord: Ordering[Int] = Ordering.Int }
object Paths {
  def use(numbers: Numbers) = { import numbers.*; succ(zero) }
  def givens(numbers: Numbers) = { import numbers.given; summon[Ordering[Int]] }
}
