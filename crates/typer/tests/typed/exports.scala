class A:
  val x: Int = 1
  def f(y: Int, z: String = "a"): Int = y
  def g[T](t: T*): Int = 1
  type M = Int
  given s: String = "s"
class B(a: A):
  export a.{x, f as h, g, M}
object C:
  val a = A()
  export a.{x, given}
object D:
  def use(b: B): Int = b.h(1) + b.g("a", "b") + C.x
object E:
  class Box(x: Int):
    def twice: Int = x * 2
    def plus(y: Int): Int = x + y
  extension (x: Int)
    private def box = Box(x)
    export box.{twice, plus as add}
  def use: Int = 3.twice + 3.add(1)
