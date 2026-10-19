class Box[T](val count: Int)
class Plain

class WellKinded[x] {
  type t = x[x]
}

object TypeArgCounts {
  type M[X] = Map[X, Int]
  val a: List[Int, String] = Nil
  val d: Map[Int] = Map()
  def g(b: Box[Int, Int, Long]): Int = 1
  def h(m: M[Int, Int]): Int = 1
  def p(x: Plain[Int]): Int = 1
}
