case class P(x: Int, y: Int)

class Greeter(g: String = "hello"):
  def greet(name: String = "world", end: String = "!"): Int = name.length + end.length

object Counter:
  var n = 0
  val arr = new Array[Int](2)

object Main:
  def side(i: Int): Int = i
  def f(a: Int, b: Int = 2, c: Int = 3): Int = a + b + c
  def run(): Unit =
    println(Greeter().greet())
    println(P(1, 2).copy(y = 5))
    println(f(c = side(1), a = side(2)))
    Counter.n += 1
    Counter.arr(side(0)) += 1
