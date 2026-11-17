object PlainIterators:
  def it(xs: List[Int]) = xs.iterator
  def first[T](xs: List[T]): T = xs.head
  def g(xs: List[Int]) =
    val i = it(xs)
    val f = first(List(i))
    f.hasNext
