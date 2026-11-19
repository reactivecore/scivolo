//> using options -source:3.2
object Refutable {
  val xs: List[Int] = List(1, 2)
  val head :: tail = xs
  val Some(y) = Option(1)
}
