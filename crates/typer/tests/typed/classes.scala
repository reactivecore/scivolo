trait Shape:
  def area: Double

abstract class Named(val name: String) extends Shape

class Circle(r: Double) extends Named("circle"):
  def area: Double = r
  def radius = r

class Box[T](val value: T, label: String = "box"):
  type Content = T
  def get: T = value

object Origin extends Named("origin") with Shape:
  def area: Double = 0.0
