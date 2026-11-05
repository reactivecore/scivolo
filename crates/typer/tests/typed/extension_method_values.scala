object E {
  extension (s: String) def twice: String = s + s
  extension (s: String) def times(n: Int): String = s * n
}
object U {
  import E.*
  val l = List("a").map(twice)
  def k(xs: List[String]) = xs.map(twice).map(_.length)
}
