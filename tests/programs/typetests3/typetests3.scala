//> using scala 3.9.0
// Type tests of unions of calls, of generic arrays, and constant ones.

object typetests3 {
  def g(n: Int): Any = if n > 0 then "s" else n

  def generic[T](x: Any): Boolean = x.isInstanceOf[Array[T]]

  def never(a: Int | Double): Boolean = a.isInstanceOf[Float | Boolean]

  def main(args: Array[String]): Unit = {
    println(g(1).isInstanceOf[String | Int])
    println(g(0).isInstanceOf[String & CharSequence])
    println(generic[String](Array("a")))
    println(generic[Int]("a"))
    println(never(1))
  }
}
