object S extends fix.G[String] {
  protected def flatten(vs: String*): String = vs.mkString("+")
}

object bridgelib {
  def main(args: Array[String]): Unit = println(S.pair("a", "b"))
}
