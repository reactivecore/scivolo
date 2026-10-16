//> using scala 3.9.0
// Type tests of unions of calls, also in a nested class, of generic and bounded arrays, and
// constant ones.

object typetests3 {
  def g(n: Int): Any = if n > 0 then "s" else n

  val field: Boolean = g(0).isInstanceOf[String | Int]

  class Nested { val v: Boolean = g(2).isInstanceOf[String | Long] }

  def generic[T](x: Any): Boolean = x.isInstanceOf[Array[T]]

  def bounded[T <: AnyRef](x: Any): Boolean = x.isInstanceOf[Array[T]]

  def never(a: Int | Double): Boolean = a.isInstanceOf[Float | Boolean]

  def main(args: Array[String]): Unit = {
    println(field)
    println(new Nested().v)
    println(g(1).isInstanceOf[String | Int])
    println(g(0).isInstanceOf[String & CharSequence])
    println(generic[String](Array("a")))
    println(generic[Int]("a"))
    println(bounded[String](Array(1, 2)))
    println(never(1))
  }
}
