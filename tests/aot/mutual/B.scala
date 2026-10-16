trait Shape:
  def area: Int
  def name: String
  def describe: String = s"$name with area $area"

case class Square(side: Int) extends Shape:
  def area: Int = side * side
  def name: String = "square"

object Registry:
  def total(shapes: List[Shape]): Int = shapes.map(_.area).sum + Circle(0).area

object Pong:
  def pong(n: Int): String = s"pong ${Ping.ping(n)}"

inline def twice(x: Int): Int = x * 2
