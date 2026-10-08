//> using scala 3.9.0
enum Shape:
  case Circle(r: Double)
  case Rect(w: Double, h: Double)
  case Empty

enum Op(val sym: String):
  case Plus extends Op("+")
  case Minus extends Op("-")

@main def enumcases(): Unit =
  val s: Shape = Shape.Rect(2, 3)
  println(s)
  println(Shape.Circle(1).ordinal)
  println(Shape.Empty)
  println(Shape.Empty.ordinal)
  println(Shape.Circle(1.5) == Shape.Circle(1.5))
  println(Shape.fromOrdinal(2))
  println(Op.Minus.sym)
  println(Op.Plus.ordinal)
  println(Op.values.map(_.sym).mkString)
