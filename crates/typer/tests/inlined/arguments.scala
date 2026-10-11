object Holder {
  println("init Holder")
  val v = 1
}

object Test {
  final val K = 5
  val w = 2
  lazy val lz: Int = { println("lz"); 3 }
  inline def ib(inline x: => Int): Int = x + x
  inline def ib0(inline x: => Int): Int = 0
  inline def twice(x: Int): Int = x + x
  inline def nothing(x: Int): Int = 1
  inline def unused(x: Int): Int = 1
  inline def sub(a: Int, b: Int): Int = a - b
  inline def p(inline x: Int): Int = x * 2
  inline def q(inline b: Boolean): String = if b then "y" else "n"
  def s(i: Int): Int = { println(i); i }
  def main(args: Array[String]): Unit = {
    lazy val loc: Int = { println("loc"); 4 }
    println(ib(s(1)))
    println(ib0(s(2)))
    println(nothing(throw new Exception("x")))
    println(twice(???))
    println(unused(lz))
    println(unused(loc))
    println(unused(Holder.v))
    println(twice(Holder.v))
    println(twice(w))
    println(sub(b = s(1), a = s(2)))
    println(p(3 + 4))
    println(p(K))
    println(q(K > 3))
    println(q(1 == 1))
    println(p(args.length))
  }
}
