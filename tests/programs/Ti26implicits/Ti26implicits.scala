import scala.language.implicitConversions
import scala.reflect.ClassTag

extension (x: Int) def m(y: Object): Int = x
def m(x: Int)(y: String): String = y * x

trait Show[-T]:
  def show(t: T): String

trait Outer:
  trait Base
  object Base:
    given Show[Base]:
      def show(t: Base): String = "base of an outer instance"

class Derived extends Outer:
  class Sub extends Base
  def shown: String = the[Show[Sub]].show(new Sub)

trait Fragable[T]:
  extension (x: T) def frags: List[String]

given Fragable[Int]:
  extension (x: Int) def frags: List[String] = List(s"i$x")
class ListFragable[A](using Fragable[A]) extends Fragable[List[A]]:
  extension (xs: List[A]) def frags: List[String] = xs.flatMap(_.frags)
given [A] => Fragable[A] => Fragable[List[A]] = ListFragable[A]

class TC
given listOps: TC => Object:
  extension (xs: List[String]) def joined: String = xs.mkString("+")

class Shown(prefix: String) extends Show[Int]:
  def show(t: Int): String = s"$prefix $t"
trait Low:
  given lowShow: Show[Int] = Shown("low")
object High extends Low:
  given highShow: Show[Int] = Shown("high")

class Ord[T](val name: String)
object Instances:
  given intOrd: Ord[Int]("int")
  given listOrd[T](using o: Ord[T]): Ord[List[T]]("list of " + o.name)

object Conversions:
  implicit object f1 extends Conversion[Int, String]:
    def apply(x: Int): String = "Int"
  implicit def f2(x: Long): String = "Long"
  def converted: String = 5

def tag[T](using t: ClassTag[T]): String = t.runtimeClass.getName

def the[T](using t: T): T = t

@main def Ti26implicits(): Unit =
  println(1.m("xx"))
  println(Derived().shown)
  println(List(1, 2).frags)
  given TC = TC()
  println(List("a", "b").joined)
  import High.given
  println(the[Show[Int]].show(3))
  import Instances.{given Ord[Int], given Ord[List[Int]]}
  println(the[Ord[List[Int]]].name)
  println(Conversions.converted)
  println(tag[Int *: String *: EmptyTuple])
