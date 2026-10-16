@main def mutual(): Unit =
  val shapes = List(Circle(1), Square(2))
  for s <- shapes do println(s.describe)
  println(Registry.total(shapes))
  println(Ping.ping(3))
  println(twice(21))

class Circle(r: Int) extends Shape:
  def area: Int = 3 * r * r
  def name: String = "circle"

object Ping:
  def ping(n: Int): String = if n == 0 then "done" else Pong.pong(n - 1)
