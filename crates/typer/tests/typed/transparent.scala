object Test {
  transparent inline def choose(b: Boolean): Any = if b then 1 else "one"
  transparent inline def twice(x: Int): Int = x + x
  transparent inline def id[T](x: T): T = x
  transparent inline def answer: Any = 42
  transparent inline def pick(inline c: Boolean, a: Int, b: String): Any = if c then a else b
  class Box(val n: Int) {
    transparent inline def get: Any = n
  }
  def side(): Int = { println("side"); 1 }
  def main(args: Array[String]): Unit = {
    val a: Int = choose(true)
    val s = choose(false)
    println(s.length)
    val t = twice(21)
    val u = twice(side())
    val i = id(List(1))
    val l: Int = answer
    println(Box(3).get + 1)
    println(pick(true, side(), "b") + a + t + u + i.head + l)
    println(later(2))
  }
  transparent inline def later(x: Int): Any = x * 2
}
object A {
  def f: Int = B.t(1) + B.u(2)
  val xs: List[Int] = B.nil
  val ys = B.id(Nil)
  val zs: List[String] = B.id(Nil)
}
object B {
  transparent inline def t(x: Int): Any = x * 3
  transparent inline def u(x: Int): Any = t(x + 1)
  transparent inline def nil: List[Nothing] = Nil
  transparent inline def id[T](x: T): T = x
}
