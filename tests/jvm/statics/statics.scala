import scala.annotation.static

class Counter
object Counter {
  println("Counter object")
  @static val start: Int = { println("start"); 10 }
  @static def twice(n: Int): Int = n * 2
  @static var count: Int = 1
  val plain = 3
}

trait Shape
object Shape {
  @static val sides: String = "four"
  @static def describe(s: String): String = "shape " + s
  @static def pair[T](t: T): List[T] = List(t, t)
}

object Named {
  val x$lzy1: Int = 11
  var bitmap$0: Int = 12
  lazy val sum: Int = x$lzy1 + bitmap$0
}

object Statics {
  def main(args: Array[String]): Unit = {
    println("main")
    println(Counter.start)
    println(Counter.twice(Counter.start))
    Counter.count = Counter.count + 4
    println(Counter.count)
    println(Counter.plain)
    println(Shape.describe(Shape.sides))
    println(Named.sum)
    println(shape().sides + shape().describe("x"))
    println(shape().pair[Int](2))
  }

  def shape(): Shape.type = { println("shape"); Shape }
}
