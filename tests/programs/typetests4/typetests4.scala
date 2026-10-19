import scala.reflect.{ClassTag, TypeTest}

trait Shapes {
  type Shape
  type Circle <: Shape
  implicit def circleTest: TypeTest[Shape, Circle]
  val Circle: CircleExtractor
  trait CircleExtractor {
    def apply(r: Int): Circle
    def unapply(c: Circle): Option[Int]
  }
  def square(side: Int): Shape

  def describe(s: Shape): String = s match {
    case Circle(r) => s"circle $r"
    case _ => "other"
  }

  def isCircle(s: Shape): Boolean = s match {
    case c: Circle => true
    case _ => false
  }

  def demo(): Unit = {
    println(describe(Circle(3)))
    println(describe(square(2)))
    println(isCircle(Circle(1)))
    println(isCircle(square(1)))
  }
}

object ClassShapes extends Shapes {
  class Shape
  final class Circle(val r: Int) extends Shape
  final class Square(val side: Int) extends Shape
  implicit object circleTest extends TypeTest[Shape, Circle] {
    def unapply(s: Shape): Option[s.type & Circle] = s match {
      case c: Circle => Some(c.asInstanceOf[s.type & Circle])
      case _ => None
    }
  }
  object Circle extends CircleExtractor {
    def apply(r: Int): Circle = new Circle(r)
    def unapply(c: Circle): Option[Int] = Some(c.r)
  }
  def square(side: Int): Shape = new Square(side)
}

object typetests4 {
  def firstOf[T: ClassTag](xs: List[Any]): Option[T] = xs match {
    case (t: T) :: _ => Some(t)
    case _ :: rest => firstOf[T](rest)
    case Nil => None
  }

  def main(args: Array[String]): Unit = {
    ClassShapes.demo()
    println(firstOf[String](List[Any](1, "two", 3.0)))
    println(firstOf[String](List[Any](1, 2)))
  }
}
