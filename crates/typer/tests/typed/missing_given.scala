object Test {
  trait Show[A]
  class Ctx
  def f(using c: Ctx): Int = 1
  def g(x: Int)(using s: Show[Int]): Int = x
  def h(using c: Ctx)(using s: Show[Ctx]): Int = 2
  val a = f
  val b = g(1)
  val c = summon[Ctx]
  val d = h
  def k(implicit c: Ctx): Int = 3
  val e = k
}
