//> using scala 3.9.0
object iarrays {
  def first[T](xs: IArray[T]): T = xs(0)

  def main(args: Array[String]): Unit = {
    val a = IArray(1, 2, 3)
    println(a(0) + a(2))
    println(a.length)
    val s: IArray[String] = IArray("x", "y")
    println(s(1) + first(s))
  }
}
