case class Box[T](x: T)
class A
class B
object Main {
  def a(x: Box[Int], y: Box[String]) = x == y
  def b(x: A, y: B) = x != y
  def c(x: Int, y: java.lang.Integer) = x == y
  def e(x: Long, y: Char) = x == y
  def f(x: A, y: Null) = x == y
  def g(x: Box[Int]) = x match {
    case Main => 1
    case _ => 2
  }
}
