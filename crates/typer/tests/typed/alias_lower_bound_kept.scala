object AliasLowerBoundKept {
  type Id[T] = T
  type Pair[T] = (T, T)
  class Box[+T](val t: T)
  def f[A](x: A): A = x
  def idw[I](i: I): Id[I] = i
  def pw[I](i: I): Pair[I] = (i, i)
  def bw[I](i: I): Box[Id[I]] = new Box(i)
  def unbox[O](b: Box[O]): O = b.t
  val a = f(idw("a"))
  val b = f(pw("a"))
  val c = unbox(bw("a"))
}
