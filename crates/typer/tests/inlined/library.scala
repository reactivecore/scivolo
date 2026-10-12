object Test {
  given Int = 3
  given String = "s"
  def main(args: Array[String]): Unit = {
    val n = args.length
    assert(n == 0)
    assert(n >= 0, "negative")
    assert(n < 10, s"big $n")
    println(summon[Int] + n)
    println(summon[String].length)
  }
}
