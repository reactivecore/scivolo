object M {
  def g[V1 >: String](v: V1): V1 = v
  val s = g(1.toString)
  def f(m: Map[Int, String], x: Int) = m.updated(x, x.toString)
}
