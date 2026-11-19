//> using options -source:future-migration
class Pair(val a: Int, val b: Int) { def plus(that: Pair) = Pair(a + that.a, b + that.b) }

object Migrating {
  val p = Pair(1, 2) plus Pair(3, 4)
  def inc(x: Int) = x + 1
  val f = inc _
  def use(implicit n: Int) = n
  val u = use(1)
}
