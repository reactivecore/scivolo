//> using scala 3.9.0
// Generic local classes, also capturing values and type parameters of their method.

object localgenerics {
  def wrap[T](t: T, n: Int): List[T] = {
    class Box[U](val u: U) {
      def twice: List[U] = List.fill(n)(u)
      def pair[V](v: V): (U, V) = (u, v)
    }
    val b = new Box(t)
    println(b.pair("x"))
    b.twice
  }

  def main(args: Array[String]): Unit = {
    class Cell[A](var a: A) { def swap(b: A): A = { val old = a; a = b; old } }
    val c = new Cell[String]("a")
    println(c.swap("b") + c.a)
    println(wrap(1, 2))
    println(wrap("s", 3))
  }
}
