object MissingParamTypes {
  val f = x => x
  val g: Int = x => x
  def h = (a, b) => a
  val k = (x: Int, y) => x + y
  val l: String = (s) => s.length
  val m = List(1, 2, 3).fold((a, b) => (a + b))
  val n = { val u = 1; if (u > 0) (v: Int) => (w => w) else (w => w) }
  val o: Option[Int] = (z) => 1
  val p = (a: Int) => a match { case 1 => (b => b); case _ => (c: Int) => c }
}
