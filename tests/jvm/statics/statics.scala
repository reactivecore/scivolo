import scala.annotation.static

class Counter
object Counter {
  println("Counter object")
  @static val start: Int = { println("start"); 10 }
  @static def twice(n: Int): Int = n * 2
  val plain = 3
}

trait Shape
object Shape {
  @static val sides: String = "four"
  @static def describe(s: String): String = "shape " + s
}

object Statics {
  def main(args: Array[String]): Unit = {
    println("main")
    println(Counter.start)
    println(Counter.twice(Counter.start))
    println(Counter.plain)
    println(Shape.describe(Shape.sides))
  }
}
