class C {
  type T = String
  def f(x: T): T = x
  def g(x: String) = x
}
object M {
  val a = new C().f("a")
  val c = new C
  val b = c.f("b")
}
