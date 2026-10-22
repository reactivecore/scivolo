object Main {
  val id = [T] => (x: T) => x
  val pair = [A, B] => (a: A, b: B) => (b, a)
  def main(args: Array[String]): Unit = {
    println(List(1, 2).map(id[Int]))
    println(List(1, 2).zip(List("a", "b")).map(pair[Int, String].tupled))
  }
}
