object Main {
  val ints = Array(1, 2, 3)
  val strings = Array("a", "b")
  val pair = Predef.ArrowAssoc(1).->("one")
  def run(): Int = ints.length + strings.length + pair._1
}
