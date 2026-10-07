case class P(x: Int, y: Int = 2)

class K(val a: Int = 1, val b: String = "b")

object Main:
  def f(a: Int, b: Int = 10, c: String = "c"): String = s"$a$b$c"
  def g[T](x: T, n: Int = 1): List[T] = List.fill(n)(x)
  def run(): Unit =
    val p = P(1)
    val q = p.copy(x = 5)
    val k = new K(b = "z")
    println(f(1))
    println(f(1, c = "d"))
    println(f(c = "e", a = 2))
    println(f(b = 3, a = 4))
    println(g("s"))
    println(q.y + k.a)
