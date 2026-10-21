import scala.util.NotGiven

class TC
case class Ctx(str: String)
case class A(x: String)
case class B(x: String)
trait Tagged[T]
case class Foo[T](value: Boolean)
trait FooLowPrio {
  implicit def fooDefault[T]: Foo[T] = Foo(true)
}
object Foo extends FooLowPrio {
  implicit def fooNotTagged[T](implicit ev: NotGiven[Tagged[T]]): Foo[T] = Foo(false)
}
object UsingClauses {
  given tc: TC()
  class Bar(using TC)
  def a = new Bar
  def b = new Bar()
  def root(using Ctx): String = summon[Ctx].str
  def narrow(using Ctx)(owner: String = root) = owner
  given Ctx("alpha")
  def c = narrow()
  given A("default")
  given B("default")
  def f(using a: A, b: B): String = a.x + b.x
  def d = f(using A("explicit"))
  def e = {
    implicit val tagged: Tagged[Int] = null
    (implicitly[Foo[Int]].value, implicitly[Foo[String]].value)
  }
  def g = (1 to 10 drop 9) == Seq(10)
  extension [L](lhs: L)(using TC)
    def minus(rhs: Int): Int = rhs
    def combine[R](rhs: R)(using TC): R = rhs
  def h = "a".minus(1) + "b".combine(2) + "c".combine(using tc)(3)
}
