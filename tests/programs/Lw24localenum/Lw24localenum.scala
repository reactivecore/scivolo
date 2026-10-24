//> using scala 3.9.0
object Lw24localenum:
  def run(k: Int): Unit =
    enum Shape:
      case Square(width: Int, height: Int) extends Shape
      case Circle(radius: Int) extends Shape
    val c = Shape.Circle(k)
    println(c)
    c match
      case Shape.Circle(r) => println(r + k)
      case _ => println("other")
    println(Shape.Square(1, 2).ordinal)

  def main(args: Array[String]): Unit = run(3)
