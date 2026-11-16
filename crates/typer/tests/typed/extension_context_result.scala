object ExtResult {
  extension (s: String) def times: Int ?=> String = s * summon[Int]
  given Int = 2
  val selected = "x".times
  val direct = times("y")
}
