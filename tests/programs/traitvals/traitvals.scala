//> using scala 3.9.0
trait Greeter:
  val greeting = "hi"
  var count = 0
  println("init")
  def greet(n: String): String =
    count += 1
    s"$greeting $n"

object G extends Greeter

class P extends Greeter:
  override val greeting = "hello"

trait Named:
  val name: String = "named"

class Both extends Greeter, Named

@main def traitvals(): Unit =
  println(G.greet("x"))
  println(G.count)
  println(P().greet("y"))
  val b = Both()
  println(b.greet(b.name))
