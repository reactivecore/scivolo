import geo.*

class Circle(r: Double) extends Shape {
  def area: Double = 3.0 * r * r
}

object User {
  def main(args: Array[String]): Unit = {
    val shapes = List(Rect.square(2.0), new Rect(1.0, 3.0), new Circle(1.0))
    println(Shapes.total(shapes))
    println(Shapes.largest(shapes).map(_.describe()))
    val c = new Cell("a")
    println(c.swap("b") + c.value)
    val buffer = scala.collection.mutable.ArrayBuffer.empty[String]
    c.into(buffer)
    println(buffer)
    println(Shapes.firstPositive(Array(-1, 0, 4)))
    println(Shapes.unit.area)
  }
}
