transparent inline def top(n: Int): Any = if n > 0 then "pos" else n

object Test {
  transparent inline def choose(b: Boolean): Any = if b then 1 else "one"
  transparent inline def outer(b: Boolean): Any = choose(!b)
  inline def plain(x: Int): Int = x + 1
  transparent inline def viaPlain(x: Int): Any = plain(x) * 2
  inline def callsTransparent(b: Boolean): Any = choose(b)
  transparent inline def lazily(x: => Int): Any = x + x
  transparent inline def pick(inline c: Boolean, a: Int, b: String): Any = if c then a else b
  extension (x: Int) transparent inline def double: Any = x * 2
  transparent inline def first[T](xs: List[T]): Option[T] = xs.headOption
  def over(i: Int): String = "int"
  def over(s: String): String = "str"
  class Counter(var n: Int) {
    transparent inline def next(): Any = { n += 1; n }
  }
  def side(): Int = { println("side"); 2 }
  def main(args: Array[String]): Unit = {
    val a = outer(false)
    println(a + 1)
    val b = viaPlain(side())
    println(b + 1)
    println(callsTransparent(true))
    println(lazily(side()))
    println(pick(b = "x", a = side(), c = false).length)
    println(3.double + 1)
    val xs: List[Int] = List(1, 2)
    println(first(xs).isEmpty)
    println(over(choose(true)))
    println(over(choose(false)))
    println(Counter(4).next() + 1)
    println(top(1).length)
    println(later(5) + 1)
  }
  transparent inline def later(x: Int): Any = x
}
