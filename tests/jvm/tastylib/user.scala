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
    val marks = summon[scala.deriving.Mirror.SumOf[Mark]]
    println((marks.ordinal(Blank), marks.ordinal(Dot(1)), Color.valueOf("Blue").ordinal))
    val pair = Pair(1, "a")
    val pairs = summon[scala.deriving.Mirror.ProductOf[Pair[Int, String]]]
    println((pair.copy(first = 2), pairs.fromProduct((3, "c")), pair == Pair(1, "a")))
    pair match { case Pair(n, s) => println(s * n) }
    println(List(Opt.Has(1), Opt.Empty).map(_.ordinal))
    println(summon[Functor[List]].map(List(1, 2))(_ + 1))
    println(Codes.zero())
    println(new Rect(1.0, 1.0) with Codes.Twice {}.describe())
  }
}
