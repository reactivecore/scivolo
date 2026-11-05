object hkerasure {
  def seq[A, CC[X] <: Iterable[X]](in: CC[A]): Int = in.size
  def heads[A, CC[X] <: Seq[X]](in: Array[CC[A]]): A = in(0).head
  def nested[A, CC[X] <: Iterable[X], DD[Y] <: CC[Y]](in: DD[A]): Int = in.size
  def main(args: Array[String]): Unit = {
    println(seq(List(1, 2)))
    println(heads(Array(Vector("a"))))
    println(nested[Int, Seq, List](List(1, 2, 3)))
  }
}
