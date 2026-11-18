object Test {
  def k(n: Int = 1, xs: String*): String = s"$n ${xs.mkString(",")}"

  def main(args: Array[String]): Unit = {
    println(k(xs = Seq("a", "b")*))
    println(k(xs = Array("x", "y")*))
  }
}
