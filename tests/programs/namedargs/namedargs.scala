//> using scala 3.9.0
case class P(x: Int, y: Int = 2)

object O:
  def g(a: Int = 1, b: String = "b"): String = s"$a$b"
  def scaled(n: Int, factor: Int = 10, offset: Int = 0): Int = n * factor + offset

class Greeter(greeting: String = "hello"):
  def greet(name: String = "world"): String = s"$greeting $name"

@main def namedargs(): Unit =
  println(O.g(b = "y"))
  println(O.g(a = 3, b = "z"))
  println(O.g())
  println(O.scaled(2))
  println(O.scaled(n = 3, factor = 2))
  val p = P(1)
  println(p)
  println(p.copy(y = 5))
  println(p.copy(x = 9))
  println(P(x = 0, y = 7))
  val greeter = Greeter()
  println(greeter.greet())
  println(Greeter("hi").greet(name = "there"))
