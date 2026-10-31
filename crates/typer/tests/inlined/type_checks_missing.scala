import scala.compiletime.testing.typeChecks

trait Eq[T]
inline def show(inline b: Boolean): Boolean = b

object TypeChecksMissing:
  implicit val eq: Eq[Int] = new Eq[Int] {}
  implicit class Ops[T](x: T):
    def ===(y: T)(implicit c: Eq[T]): Boolean = x == y
  def f(x: Int): Int = x
  val a = show(typeChecks("missing + 3"))
  val b = show(typeChecks("missingFun(3)"))
  val c = show(typeChecks("Missing(3)"))
  val d = show(typeChecks("5 === 5"))
  val e = show(typeChecks("5.6 === 7.7"))
  val g = show(typeChecks("f(3)"))
