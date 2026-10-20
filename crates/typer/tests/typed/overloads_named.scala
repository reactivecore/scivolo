object OverloadsNamed {
  def f(xs: String*): Int = xs.length
  def f(a: Int = 1, b: String = "b"): Int = a
  def g(count: Int = 1, x: Boolean = true)(y: Double): Double = y
  def g(y: Double): Double = y

  val R = "(a)(b)?".r

  def test(s: String): Unit = {
    f(b = "c")
    f(a = 2)
    g(count = 5)(1.0)
    g(1.0)
    val a = Array("1")
    val a2 = Array(a*)
    val bs = Array(true)
    val bs2 = Array(bs*)
    s match {
      case R(x, y) => ()
      case _ => ()
    }
  }
}
