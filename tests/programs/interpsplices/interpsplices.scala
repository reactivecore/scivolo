class GlobInterpolator(sc: StringContext) {
  def g(parts: Any*) = new StringContext(sc.parts*).s(parts*)
  def stable(parts: Any*) = sc.s(parts*)
  def list(parts: Any*) = new StringContext(sc.parts*).s(parts.toList*)
  def array(parts: Array[Any]) = new StringContext(sc.parts*).raw(parts*)
  def single(a: Int) = new StringContext(sc.parts*).s(a)
}

@main def interpsplices(): Unit = {
  val g = new GlobInterpolator(StringContext("a", "\\tb"))
  println(g.g(1))
  println(g.stable(2))
  println(g.list(3))
  println(g.array(Array(4)))
  println(g.single(5))
}
