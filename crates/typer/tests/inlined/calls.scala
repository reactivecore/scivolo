inline def top(x: Int): Int = x * 2

object Obj {
  inline def sq(x: Int): Int = x * x
}

object Test {
  inline def twice(x: Int): Int = x + x
  inline def pick(inline c: Boolean, a: Int, b: Int): Int = if c then a else b
  inline def byName(x: => Int): Int = x + x
  inline def gen[T](x: T): T = x
  inline def two(a: Int)(b: Int): Int = a - b
  inline def local(x: Int): Int = { val y = x + 1; y * y }
  inline def choose(inline flag: Boolean, a: Int): Int = if flag then a else 0
  inline def ignore(x: Int): Int = 7
  inline def greet(name: String): String = "hi " + name
  inline def unit(x: Int): Unit = println(x)
  inline def label(x: Int, inline g: Int => String): String = g(x)
  def side(): Int = { println("side"); 1 }
  def f: Int => String = _.toString
  def main(args: Array[String]): Unit = {
    val n = args.length
    println(twice(21))
    println(twice(side()))
    println(pick(args.isEmpty, 1, side()))
    println(top(n))
    println(Obj.sq(3))
    println(Obj.sq(n))
    println(byName(side()))
    println(gen[String]("s"))
    println(gen(n))
    println(two(n)(side()))
    println(local(n))
    println(choose(true, n))
    println(choose(false, side()))
    println(ignore(side()))
    println(ignore(n))
    println(greet("bob"))
    println(greet(args.mkString))
    println(label(n, f))
    unit(n)
  }
}
