import scala.language.implicitConversions

trait R { def r: String }
object R { implicit class SR(s: String) extends R { def r = s } }
trait W { def w: String }
trait LowW { implicit def readableWritable[T](t: T)(implicit f: T => R): W = new W { def w = f(t).r } }
object W extends LowW { implicit class BA(a: Array[Byte]) extends W { def w = "bytes" } }
trait Src { def get: W }
object Src { implicit class WS[T](s: T)(implicit f: T => W) extends Src { def get = f(s) } }

object Views {
  def over(p: Src): String = p.get.w
  def a = over("hi")
  def b = over(Array[Byte](1))
}

case class A(b: Int, c: String)
object CaseCopy {
  implicit def s2i(s: String): Int = s.length
  implicit def toA[T](t: T)(implicit f: T => Int): A = A(f(t), t.toString)
  def a = "asdf".copy(b = 3)
}

class Animal
class Dog extends Animal
object Ranking {
  implicit def fromAnimal(a: Animal): String = "animal"
  implicit def fromDog(d: Dog): String = "dog"
  def f(implicit g: Dog => String) = g(new Dog)
  def a = f
}

object TwoParams {
  implicit def add(x: Int, y: Int): Int = x + y
  def f(implicit g: (Int, Int) => Int) = g(1, 2)
  def a = f
}

object ValueAndMethod {
  implicit val length: String => Int = _.length
  implicit def size(s: List[Int]): Int = s.size
  def f[T](t: T)(implicit g: T => Int) = g(t)
  def a = (f("abc"), f(List(1, 2)))
}

object InlineCandidate {
  implicit inline def uhOh[A](value: A): A = compiletime.error("not eta-expanded")
  def a = summon[Function1[Int, Int]]
}

class Ev
object MisfitInClosure {
  implicit def needsEv(s: String)(implicit e: Ev): Int = 0
  implicit def size(s: String): Int = s.length
  def f(implicit g: String => Int) = g("abc")
  def a = f
}

class Cap[T]
object Cap { implicit def any[T]: Cap[T] = new Cap[T] }
class Wrap[T](val x: List[T])
object ConversionResult {
  implicit def conv[T](x: List[T])(implicit e: Cap[T]): Wrap[T] = new Wrap(x)
  def a: Wrap[Any] = List(1, 2)
}

object OpenValueWinner {
  def viewed[B](x: Int)(implicit ev: Int => B): B = ev(x)
  def a = viewed(3)
}
