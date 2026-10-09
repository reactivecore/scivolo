object Main {
  def f(x: Int): Int = x
  def f(x: String, y: Int = 1): Int = y
  def g(x: => Int): Int = x
  def g(x: String): Int = 0
  def h(x: Int, y: Int = 2): Int = x + y
  def h(x: String): Int = 1
  def k(a: Int): Int = a
  def k(a: Int, b: Int = 0): Int = b
  val a = f(1)
  val b = f("s")
  val c = f("s", 2)
  val d = g(1)
  val e = g("x")
  val m = h(1)
  val n = h("x")
  val o = k(1)
  val sb = new StringBuilder
  val p = sb.append("x")
  val q = "abc".indexOf("b")
  val r = List(1, 2).mkString(",")
  val s = Some(1).getOrElse(0)
}
