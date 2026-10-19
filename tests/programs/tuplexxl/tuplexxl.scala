//> using scala 3.9.0
object tuplexxl {
  type T23 = (Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, String)

  def last(t: T23): String = t match {
    case (a, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, s) => s * a
  }

  def size(x: Any): Int = x match {
    case (_, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _) => 24
    case (_, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _) => 23
    case _ => 0
  }

  def main(args: Array[String]): Unit = {
    val t = (2, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, "ab")
    println(last(t))
    println(t)
    val u: Tuple = (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24)
    println(size(t) + size(u) + size(1))
    println(u.isInstanceOf[*:[?, ?]])
    println(t.productArity)
    val l: L23 = (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, "c")
    println(l)
    println(same(t).productArity)
  }

  type L23 = (Long, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, String)

  def same(t: T23): T23 = t match {
    case p @ (_, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _) => p
  }
}
