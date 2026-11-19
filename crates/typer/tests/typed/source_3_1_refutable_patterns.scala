//> using options -source:3.1
object Refutable {
  val xs: List[Int] = List(1, 2)
  val head :: tail = xs
  val Some(y) = Option(1)
  def twice(f: Int => Int) = f
  val g = twice _
}
