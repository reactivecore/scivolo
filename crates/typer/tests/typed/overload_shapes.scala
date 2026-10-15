object Shapes {
  def f(x: Int, g: Int => Int): Int = g(x)
  def f(x: String, g: String => String): String = g(x)
  def g(h: Int => Int): Int = h(1)
  def g(h: (Int, Int) => Int): Int = h(1, 2)
  def h(k: Int => Int): Int = k(1)
  def h(k: Int => String): String = k(1)
}

object Main {
  val m = Map(1 -> 2)
  val a = Shapes.f("abc", { case s: String => s })
  val b = Shapes.f(1, x => x + 1)
  val c = Shapes.g(x => x + 1)
  val d = Shapes.g((x, y) => x + y)
  val e = Shapes.h(x => x.toString)
  val n = m.map { case (k, v) => (k + 1, v.toString) }
  val s = m.map((k, v) => k + v)
}
