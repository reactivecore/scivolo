object PolySelf {
  def nest[T](n: Int, x: T): List[T] =
    if (n == 0) List(x) else nest[List[T]](n - 1, List(x)).head
}
