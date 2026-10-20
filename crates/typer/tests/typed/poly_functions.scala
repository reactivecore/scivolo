object Main {
  val first = [T] => (ts: List[T]) => ts.headOption
  val pair = [A, B] => (a: A, b: B) => (b, a)
  def main(args: Array[String]): Unit = {
    println(first(List(1, 2, 3)))
    println(pair(1, "one"))
  }
}
