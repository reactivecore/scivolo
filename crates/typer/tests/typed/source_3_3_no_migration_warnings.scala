//> using options -source:3.3
class Pair(val a: Int, val b: Int) { def plus(that: Pair) = Pair(a + that.a, b + that.b) }

object Before34 {
  val p = Pair(1, 2) plus Pair(3, 4)
  def inc(x: Int) = x + 1
  val f = inc _
}
