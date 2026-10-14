//> using scala 3.9.0
import scala.reflect.ClassTag

sealed trait Base
case object A extends Base
case object B extends Base
class V(val x: Int) extends AnyVal
class Outer(s: String):
  class In:
    override def toString = "In of " + s
  object In:
    implicit def inToString(i: In): String = s + "!"
  implicit def mk: In = new In
trait Monoid[T]:
  extension (x: T) def combine(y: T): T
  def unit: T
class K
object K:
  extension (k: K) def twice: Int = 2

object Tags:
  object Foo
  val foo = "foo"
  implicit object Pos:
    extension (x: Int) def positive: Boolean = x > 0
  given StringMonoid: Monoid[String] with
    extension (x: String) def combine(y: String): String = x.concat(y)
    def unit: String = ""
  def tag[T](using t: ClassTag[T]): ClassTag[T] = t
  def nested[T: ClassTag](x: T): Array[Array[T]] = Array(Array(x))
  def sum[T: Monoid](xs: List[T]): T = xs.foldLeft(implicitly[Monoid[T]].unit)(_.combine(_))

  def run(): Unit =
    println(tag[Array[Int]])
    println(tag[Option[Int]])
    println(tag[(String, Int)])
    println(tag[A.type])
    println(tag[Array[Array[String]]])
    println(tag[String | Int])
    println(tag[Base & Product])
    println(tag[V])
    println(tag[Unit])
    println(tag[Any])
    val same = reflect.classTag[Int | Int]
    println(same)
    val ab = Array(A, B)
    println(ab(1))
    println(Array(Array(1), List(1)).length)
    val n = nested("x")
    println(n(0)(0))
    println(implicitly[ValueOf[1]].value)
    println(implicitly[ValueOf["s"]].value)
    println(implicitly[ValueOf[Unit]].value)
    println(implicitly[ValueOf[Foo.type]].value == Foo)
    println(implicitly[ValueOf[foo.type]].value)
    val o = new Outer("OK")
    val s: String = new o.In
    println(s)
    println(summon[o.In])
    println(1.positive)
    println("a".combine("b"))
    println(sum(List("x", "y")))
    println((new K).twice)

@main def classtags(): Unit = Tags.run()
