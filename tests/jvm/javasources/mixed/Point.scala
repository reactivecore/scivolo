package mixed

@Tag case class Point(x: Int, y: Int):
  def show: String = s"($x, $y)"
  def color: Circle.Color = if x > y then Circle.Color.RED else Circle.Color.GREEN

class Square(side: Double) extends Shape:
  def area(): Double = side * side
  def name(): String = "square"
