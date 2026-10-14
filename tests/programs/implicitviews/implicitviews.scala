//> using scala 3.9.0
import scala.language.implicitConversions
import scala.util.NotGiven

class Box(val s: String)
class TC(val n: Int)
case class Ctx(str: String)
trait Tagged[T]
case class Foo[T](value: Boolean)
trait FooLowPrio:
  implicit def fooDefault[T]: Foo[T] = Foo(true)
object Foo extends FooLowPrio:
  implicit def fooNotTagged[T](implicit ev: NotGiven[Tagged[T]]): Foo[T] = Foo(false)

object Views:
  given Conversion[Int, Box] with
    def apply(i: Int): Box = new Box("box" + i)
  given boolBox(using tc: TC): Conversion[Boolean, Box] with
    def apply(x: Boolean): Box = new Box(s"$x${tc.n}")
  implicit def longName(x: Long): String = "long" + x
  given TC(7)
  given Ctx("alpha")
  class Bar(using tc: TC):
    def n = tc.n
  def root(using c: Ctx): String = c.str
  def narrow(using Ctx)(owner: String = root) = owner
  def both(using c: Ctx, tc: TC): String = c.str + tc.n

  def run(): Unit =
    val b: Box = 1
    println(b.s)
    println((true: Box).s)
    val s: String = 5
    println(s)
    println(new Bar().n)
    println((new Bar).n)
    println(narrow())
    println(narrow("beta"))
    println(both(using Ctx("explicit")))
    implicit val tagged: Tagged[Int] = null
    println(implicitly[Foo[Int]].value)
    println(implicitly[Foo[String]].value)
    println((1 to 3 drop 2) == Seq(3))

@main def implicitviews(): Unit = Views.run()
