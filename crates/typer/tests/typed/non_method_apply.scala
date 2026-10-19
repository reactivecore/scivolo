class Box(val count: Int)

object NonMethodApply {
  val n = 1
  val b = new Box(1)
  val a = n(2)
  val c = b.count(3)
  val d = b(1)
  def f(p: Boolean) = p(1)
  def g = {
    val loc = 2.0
    loc(1, 2)
  }
}
