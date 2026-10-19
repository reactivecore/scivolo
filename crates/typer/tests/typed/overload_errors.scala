def top(x: Int): Int = x
def top(y: => Boolean): Int = 1

class C {
  def m(x: Int): Int = x
  def m(xs: List[String]): Int = 0
  def use = m("s")
}

trait T {
  def k(x: Int): Int = x
  def k(x: String): Int = 0
  def use = k(1.5)
}

object O {
  def f(x: Int): Int = x
  def f(x: String): String = x
  def g(x: Int, y: Int): Int = x
  def g(x: String, y: Int): String = x
  def bar(a: Int, b: Long): Int = 1
  def bar(a: Long, b: Int): Int = 2
  val s = "x"
  val a = f(true)
  val b = g(1.0, 2)
  val c: Int = f(2.0)
  val d = top(s)
  val e = bar(1, 1)
  def h(z: List[Int]) = f(z)
}
