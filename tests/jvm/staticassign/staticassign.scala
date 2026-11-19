import scala.annotation.static

class Counter
object Counter:
  @static var a: Int = 1
  @static var s: String = "x"

object Staticassign:
  def counter(): Counter.type =
    println("counter")
    Counter

  def main(args: Array[String]): Unit =
    Counter.a += 5
    println(Counter.a)
    counter().a += 2
    println(Counter.a)
    counter().s = "y"
    Counter.s += "z"
    println(Counter.s)
