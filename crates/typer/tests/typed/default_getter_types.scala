case class B[T](b: List[Int]) {
  var s: B[Int] = null
  def cpy[X](b: List[Int] = b): B[X] = new B[X](b)
  def again = s.cpy()
}

object Main {
  def read[S](baz: Any = 0): Option[S] = None
  def pick[T](x: T, n: Int = 1): List[T] = List.fill(n)(x)
  def test[T](i: Int, config: String = ""): Int = 1
  def test[T](i: String): Int = 2
  def bound[F: Ordering](a: Boolean, w: Int = 0): Int = w
  def bound[F: Ordering]: Int = bound[F](false)
  def main(args: Array[String]): Unit = {
    println(read[Int]())
    println(pick("s"))
    println(test[Int](1))
  }
}
