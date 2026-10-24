//> using scala 3.9.0
import scala.annotation.static

class Counter
object Counter:
  println("init Counter")
  @static def twice(i: Int): Int = i * 2
  @static def sum(n: Int): Int = List(1, 2).map(x => x + n).sum
  def viaObject: Int = twice(3)

trait Greeter:
  given greeting: String
  def greet(name: String): String = hello(name)
  def hello(name: String)(using g: String): String = g + ", " + name

object English extends Greeter:
  given greeting: String = "Hello"

object Inlines:
  inline def three: Int = 3

object Exported:
  export Inlines.three

@main def Nb24statics(): Unit =
  println(Counter.twice(4))
  println(Counter.sum(4))
  println(Counter.viaObject)
  println(English.greet("Ann"))
  println(Exported.three)
