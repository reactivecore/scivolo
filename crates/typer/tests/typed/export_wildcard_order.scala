class P:
  def zeta: Int = 1
  def add(x: Int): Int = x
class Q extends P:
  def add(x: String): String = x
  def add(x: Int, y: Int): Int = x + y
  def beta[B](b: B, n: Int = 2): List[B] = List.fill(n)(b)
  type Alpha = Int
object R:
  val q = Q()
  val xs: List[String] = Nil
  export q.*
  export xs.{++, prepended}
