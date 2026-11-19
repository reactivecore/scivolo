object AliasArgsCompared {
  class CoContra[+T, -S](val t: T)
  type Id[T] = T
  def unwrap[Outer](inv: CoContra[Outer, Outer]): Outer = inv.t
  def wrap[Inner](i: Inner): CoContra[Id[Inner], Id[Inner]] = new CoContra(i)
  val a = unwrap(wrap("a"))

  opaque type Tagged[X, Y] = X
  def untag[S, T](st: Tagged[S, T]): S = st
  type @@[S, T] = Tagged[S, T]
  def u[S, T](st: S @@ T): S = untag(st)

  type Lifted[A] = Exception | A
  def point[O](o: O): Lifted[O] = o
  extension [O, U](o: Lifted[O]) def map(f: O => U): Lifted[U] = ???
  val p = point("a").map(_ => if true then 1 else Exception())
}
