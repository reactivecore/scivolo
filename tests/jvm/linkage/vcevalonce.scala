class Cell[A](val a: A) extends AnyVal {
  def map[B](f: A => B): Cell[B] = new Cell(f(a))
}

object VcEvalOnce {
  def mk(i: Int): Cell[Int] = new Cell(i)

  def mapped: Cell[Int] = mk(20).map(_ + 1)

  def scala2: valueclasses.Box[Int] = new valueclasses.Box(20).map(_ + 1)
}
