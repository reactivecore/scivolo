trait Ord[-T] { def less(x: T, y: T): Boolean }
trait Source[+T] { def get: T }
class Low
object Low { implicit val low: Low = new Low }
class Medium extends Low
object Medium { implicit val medium: Medium = new Medium }
class Foo[T](val i: Int)
object Foo {
  def apply[T](implicit fooT: Foo[T]): Int = fooT.i
  implicit def foo[T](implicit priority: Low): Foo[T] = new Foo[T](0)
  implicit def foobar[T](implicit priority: Low): Foo[Bar[T]] = new Foo[Bar[T]](1)
}
class Bar[T]
object Bar {
  implicit def foobar[T](implicit priority: Medium): Foo[Bar[T]] = new Foo[Bar[T]](3)
}
class A
class B extends A
class C extends A
case class Box(n: Int)
class Holder(n: Int) { implicit val box: Box = Box(n) }

object Contravariant {
  implicit def f: String => String = x => "f"
  implicit def g: Object => String = x => "g"
  def h(implicit x: String => String) = x("")
  implicit val anyIsOrd: Ord[Any] = new Ord[Any] { def less(x: Any, y: Any) = false }
  implicit val intIsOrd: Ord[Int] = new Ord[Int] { def less(x: Int, y: Int) = x < y }
  def less[T: Ord](x: T, y: T) = implicitly[Ord[T]].less(x, y)
  def a = (h, less(1, 2))
}
object Widening {
  implicit def _1: Long = 1L
  implicit def _2: Int = 0
  def a = implicitly[AnyVal]
}
object Givens {
  given A = A()
  given B = B()
  given C = C()
  def f(using a: A, b: B, c: C) = a
  def a = f
}
object GivenObjects {
  given anySource: Source[Any] with { def get = 1 }
  given Source[Int] = new Source[Int] { def get = 2 }
  def a = summon[Source[Any]]
}
object GivenClasses {
  given listSource[T]: Source[List[T]] with { def get = Nil }
  given Source[Seq[Int]] = new Source[Seq[Int]] { def get = Seq(3) }
  def a = summon[Source[Seq[Int]]]
}
object ImplicitClauses {
  def a = (Foo[Int], Foo[Bar[Int]])
}
object Shadowing {
  object m1 { implicit val box: Box = Box(1) }
  object m2 { implicit val box: Box = Box(2) }
  def local(holder: Holder) = {
    import holder._
    implicit val box: Box = Box(2)
    implicitly[Box]
  }
  def named = {
    import m1._
    import m2.box
    implicitly[Box]
  }
  def namedFirst = {
    import m2.box
    import m1._
    implicitly[Box]
  }
}
object OpenVars {
  implicit def anyList[T]: List[T] = Nil
  implicit def intList: List[Int] = 42 :: Nil
  def foo[T](implicit x: T) = x
  def a = foo[List[Int]]
}
