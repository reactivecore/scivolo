object Test {
  inline def twice(x: Int): Int = x + x
  inline def inferred(x: Int) = x * 2
  inline def id[T](inline x: T): T = x
  inline def pick(inline c: Boolean, a: Int, b: Int): Int = if c then a else b
  inline def lazyOr(inline a: Boolean, b: Boolean): Boolean = a || b
  inline val limit = 10
  extension (s: String) inline def shout: String = s.toUpperCase
  class C(val n: Int) {
    inline def plus(m: Int): Int = n + m
  }
  def side(): Int = { println("side"); 1 }
  def main(args: Array[String]): Unit = {
    println(twice(21))
    println(inferred(3))
    println(id(side()))
    println(id[String]("x"))
    println(pick(b = 3, a = 4, c = args.isEmpty))
    println(lazyOr(b = side() > 0, a = args.isEmpty))
    println(limit)
    println("hi".shout)
    println(C(2).plus(side()))
    println(locally { 5 })
  }
}
