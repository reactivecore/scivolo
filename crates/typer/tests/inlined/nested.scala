inline def inline1(inline f: Int => Int): Int => Int = i => f(1)
inline def inline2(inline f: Int => Int): Int = f(2) + 3
def test: Int = inline2(inline1(2.+))

object Test {
  inline def inc(x: Int): Int = x + 1
  inline def inc2(x: Int): Int = inc(inc(x))
  inline def sq(x: Int): Int = x * x
  inline def sqInc(x: Int): Int = sq(inc(x))
  inline def fact(n: Int): Int = if n == 0 then 1 else n * fact(n - 1)
  inline def g(n: Int): Int = n * g0(n)
  inline def g0(n: Int): Int = 5
  inline def app(f: Int => Int, x: Int): Int = f(x)
  inline def appI(inline f: Int => Int, x: Int): Int = f(x) + f(x)
  inline def twice(x: => Int): Int = x + x
  def side(): Int = { println("side"); 2 }
  def main(args: Array[String]): Unit = {
    val n = args.length
    println(inc2(n))
    println(inc2(3))
    println(sqInc(side()))
    println(fact(3))
    println(g(2))
    println(app(_ + 1, n))
    println(app(y => { val z = y * 2; z + 1 }, 3))
    println(appI(_ * 3, n))
    println(twice({ val q = n + 1; q }))
  }
}
