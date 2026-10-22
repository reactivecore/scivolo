//> using scala 3.9.0
case class Point(x: Int) {
  def copy(x: Int = 9): Point = Point(x + 100)
}

case class Pair(a: Int, b: Int) {
  def copy(a: Int): Pair = Pair(2 * a, b)
}

case class Named(name: String)

object Named {
  def apply(name: String): Named = new Named(name.toUpperCase)
}

object usercopy {
  def main(args: Array[String]): Unit = {
    println(Point(1).copy())
    println(Point(1).copy(2))
    println(Pair(1, 2).copy(3))
    println(Named("abc"))
  }
}
