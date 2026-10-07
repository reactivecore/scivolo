sealed trait Shape
case class Circle(r: Int) extends Shape
case class Square(s: Int) extends Shape

object Main {
  def side(i: Int): Int = i
  val options = List(Some(1), None)
  val shapes = List(Circle(1), Square(2))
  val pairs = (side(1) -> "one") :: List(2 -> "two")
  def run(): Int = options.length + shapes.length + pairs.length
}
