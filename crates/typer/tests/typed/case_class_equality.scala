case class Box[A](value: A)
case class P(x: Int)

object Main {
  def a: Boolean = Box(1) == Box(1)
  def b: Boolean = Box(1).equals(Box(1))
  def c: Boolean = P(1).equals(P(2))
  def d: Boolean = P(1) != P(2)
  def e(x: Box[Int]): Boolean = x == Box(2)
  def f(x: Option[Int]): Boolean = x == None
  def g(x: List[Int]): Boolean = x == Nil
}
