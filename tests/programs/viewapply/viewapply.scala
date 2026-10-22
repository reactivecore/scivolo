//> using scala 3.9.0
object viewapply {
  def main(args: Array[String]): Unit = {
    val words = List("apple", "berry", "cherry")
    println(words.map(_(0)))
    println(words.groupBy(_(1)).keys.toList.sorted)
  }
}
