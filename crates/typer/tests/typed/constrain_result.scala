object Main {
  def mk[A, B]: Either[A, B] = null
  def opt[A]: Option[A] = None
  val a: Map[Int, Int] = Map.empty
  val b: Either[Int, Int] = mk
  def c: Option[String] = opt
  val d = List(opt[Int], opt)
  def e(x: Option[Int]): Int = 0
  def f = e(opt)
  val g: List[Int] = List.empty
  val h: Seq[String] = Nil ++ List.empty
}
