object Overloads {
  def r(p: PartialFunction[Int, String]): String = p(1)
  def r(p: PartialFunction[Int, Int]): Int = p(1)
  def p1(g: PartialFunction[Int, Int]): String = "PF"
  def p1(g: Int => Int): String = "F1"
  def e1(g: Int => Int, x: Int): String = "F1"
  def e1(g: String => Int, x: String): String = "S1"
}

object Main {
  val positive: PartialFunction[Int, Int] = { case x if x > 0 => x }
  val p: PartialFunction[Int, Int] = x => x match { case 1 => 1 }
  val q: PartialFunction[Int, String] = _.toString
  val evens = List(1, 2, 3).collect { case x if x % 2 == 0 => x.toString }
  val chosen = Overloads.r { case 1 => "one" }
  val partial = Overloads.p1 { case 1 => 2 }
  val function = Overloads.p1(x => x)
  val second = Overloads.e1(x => x.length, "a")
  val small: PartialFunction[Int, String] = { case x if x < 3 => "small" }
  val len: String => Int = s => s.length
  val a = small andThen (_.length)
  val b = small andThen (s => s.length)
  val c = small andThen { case s => s.length }
  val d = small andThen len
}
