class Circle(r: Double) extends basics.Shape {
  def area: Double = 3.0 * r * r
}

object Extends {
  def main(args: Array[String]): Unit = println(new Circle(1.0).name)
}
