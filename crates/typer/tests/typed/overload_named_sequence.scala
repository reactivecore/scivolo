object OverloadNamedSequence {
  def f(a: Int = 1, xs: String*): String = s"$a ${xs.mkString(",")}"
  def f(b: Boolean): String = s"b $b"

  def g(a: Int, b: String = "d", xs: Int*): String = s"$a $b ${xs.sum}"
  def g(s: String): String = s

  val ys = List("p", "q")
  def t1 = f(xs = ys*)
  def t2 = f(a = 2, xs = ys*)
  def t3 = g(1, xs = List(1, 2, 3)*)
  def t4 = g(a = 4, xs = Seq(5)*)
}
