case class Plus(a: Int, b: Int)

object Test {
  def curry2[A, B, C](f: (A, B) => C): A => B => C = a => b => f(a, b)
  def main(args: Array[String]): Unit = {
    println(curry2(Plus)(1)(2))
  }
}
