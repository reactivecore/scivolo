object Overloads {
  def r(p: PartialFunction[Int, String]): String = p(1)
  def r(p: PartialFunction[Int, Int]): Int = p(1)
}

object Main {
  val positive: PartialFunction[Int, Int] = { case x if x > 0 => x }
  val p: PartialFunction[Int, Int] = x => x match { case 1 => 1 }
  val q: PartialFunction[Int, String] = _.toString
  val evens = List(1, 2, 3).collect { case x if x % 2 == 0 => x.toString }
  val chosen = Overloads.r { case 1 => "one" }
}
