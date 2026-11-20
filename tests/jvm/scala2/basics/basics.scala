import basics.*

object Basics {
  def main(args: Array[String]): Unit = {
    val b = new Box(20)
    b.count = 3
    println(b.map(_ + 1).get)
    println(b.count)
    println(Box.of("s").value)
    println(Box.zero)
    val sq = new Square(3.0)
    val s: Shape = sq
    println(s.area)
    println(s.name)
    val lib = new Lib
    println(lib.twice(4))
    println(lib.twice("ab"))
    println(lib.max("a", "b"))
    println(lib.sum(List(1, 2, 3)))
  }
}
