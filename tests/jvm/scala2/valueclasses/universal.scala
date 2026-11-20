import valueclasses.*

class Tri(val b: Double) extends Shape { def area: Double = b / 2 }

class Circle(val r: Double) extends AnyVal with Shape { def area: Double = r * 3 }

object Universal {
  def main(args: Array[String]): Unit = {
    println(new Tri(4).describe)
    println(new Circle(2).describe)
    println(Ops.shapes(new Circle(1)))
    val arr: Array[Square] = Array(new Square(2))
    println(arr(0).area)
    println(Ops.id(new Box(3)).get)
  }
}
