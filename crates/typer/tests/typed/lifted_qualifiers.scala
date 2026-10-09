object Main {
  def a = List(1, 2).iterator.sliding(2)
  def b = List(1, 2, 3).iterator.sliding(2, 1).toList
  def c: Iterator[Seq[Int]] = Iterator(1, 2).grouped(1)
}
