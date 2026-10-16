//> using scala 3.9.0
// Extractors and objects of an enclosing class used in inner classes and nested objects.

class Shapes(scale: Int) {
  abstract class Shape {
    def area: Int = this match {
      case Shape.Square(s) => s * s * scale
      case Shape.Rect(w, h) => w * h * scale
      case _ => 0
    }
  }
  object Shape {
    case class Square(s: Int) extends Shape
    case class Rect(w: Int, h: Int) extends Shape
    def unit: Shape = Square(1)
  }
  class Box(s: Shape) {
    def describe: String = s match {
      case Shape.Square(n) => "square " + n
      case _ => "other"
    }
  }
}

object outerpatterns {
  def main(args: Array[String]): Unit = {
    val sh = new Shapes(2)
    println(sh.Shape.Square(3).area + sh.Shape.Rect(2, 5).area)
    println(new sh.Box(sh.Shape.unit).describe)
    println(new sh.Box(sh.Shape.Rect(1, 1)).describe)
  }
}
