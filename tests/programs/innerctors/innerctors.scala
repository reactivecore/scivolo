//> using scala 3.9.0
// Secondary constructors of inner classes reading their enclosing instance.

class Outer(val base: Int) {
  def twice(n: Int): Int = n * 2
  class Inner(val a: Int, val b: Int) {
    def this(a: Int) = {
      this(a, base)
      println("inner " + twice(a))
    }
    def this() = this(twice(base))
    override def toString = s"Inner($a, $b, $base)"
  }
}

object innerctors {
  def main(args: Array[String]): Unit = {
    val o = new Outer(5)
    println(new o.Inner(1, 2))
    println(new o.Inner(3))
    println(new o.Inner())
  }
}
