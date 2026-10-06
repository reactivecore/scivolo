trait Greeter:
  def greet(n: String): String

abstract class Base(val k: Int):
  def twice: Int

enum Color:
  case Red, Green

enum Shape:
  case Circle(r: Double)
  case Square(s: Double)

trait A
trait B

object Main:
  def run(): Unit =
    val g = new Greeter:
      def greet(n: String) = "hi " + n
    val b = new Base(3) { def twice = k * 2 }
    val r = new Runnable { def run(): Unit = println("r") }
    val ab = new A with B {}
    println(g.greet("x") + b.twice)
    r.run()
    println(Color.Red.ordinal + Color.values.length)
    val s: Shape = Shape.Circle(1.0)
    val area = s match
      case Shape.Circle(r) => r * r
      case Shape.Square(x) => x * x
    println(area)
