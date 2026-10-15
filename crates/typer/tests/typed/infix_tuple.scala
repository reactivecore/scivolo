import scala.collection.mutable.ListBuffer

object Test {
  class B {
    infix def put(a: Int, b: Int): Int = a + b
    infix def put(a: String, b: Int): String = a * b
  }
  def main(args: Array[String]): Unit = {
    val b = new B
    println(b put (4, 4))
    println(b put ("x", 2))
    val xs = List(1, 2, 3)
    println(xs.slice(1, 2) ++ (xs slice (0, 1)))
    val buf = ListBuffer.empty[(Int, Int)]
    buf addOne (1, 2)
    println(buf)
  }
}
