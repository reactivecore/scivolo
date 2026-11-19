class Test {
  def nothing: Nothing = ???
  def m(xs: Any*): Int = xs.length
  val a = List(null*)
  val b = List[Null](null*)
  val c = List[String](null*)
  def d = List[Int](nothing*)
  def e = m(nothing*)
}
