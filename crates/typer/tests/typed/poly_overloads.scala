import scala.language.implicitConversions

class Apply { def apply[A](x: Int) = 1 }
object A {
  def a[F] = new Apply
  def a[F](x: String) = 0
}

class Show[T](val i: Int)
class Generic
object Generic {
  implicit val gen: Generic = new Generic
}
object B {
  def bar[T <: AnyVal](x: List[T]): Int = 1
  def bar[T <: String](x: Option[T]): Int = 2
}

class Unit2
object Hour extends Unit2
case class Measure(scalar: Double, unit: Unit2) {
  def *(newUnit: Unit2) = Measure(scalar, newUnit)
}

object Test {
  implicit def double2Measure(scalar: Double): Measure = Measure(scalar, Hour)

  def main(args: Array[String]): Unit = {
    val xs = Array.ofDim[Int](5)
    val m = Array.ofDim[Int](2, 3)
    val cs = Array.fill[Char](3)('-')
    val a = A.a[String][Int](3)
    val b = B.bar[String](Some("x"))
    val h = 3.5 * Hour
    val d = 5 + BigDecimal(3)
  }
}
