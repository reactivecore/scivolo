//> using scala 3.9.0
// Field lambdas of an object after a nested object and a method with lambdas.

object M {
  object S { val f = () => 1 }
  def twice(n: Int) = List(n).map(k => k * 2).head
  val g = (n: Int) => n + 1
}

object lowerfixes2 {
  def main(args: Array[String]): Unit = {
    println(M.S.f() + M.g(2) + M.twice(3))
  }
}
