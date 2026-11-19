trait IntFn { def run(x: Int): String }

object H {
  def h(x: Int): String = "int"
  def h(x: Long): String = "long"
  val g = h
  val k: IntFn = h
}
