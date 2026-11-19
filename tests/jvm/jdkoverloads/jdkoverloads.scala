object Jdkoverloads {
  def main(args: Array[String]): Unit = {
    val xs = List("A", "b")
    val lower = xs.mapConserve(_.toLowerCase)
    println(lower)
    println(xs.mapConserve(_.trim) eq xs)
  }
}
