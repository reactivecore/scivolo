class P[+T]

object Ops {
  extension [T](p: P[T]) def map1[V](f: T => V): P[V] = new P[V]
  extension [T](inline p: P[T]) inline def map2[V](inline f: T => V): P[V] = new P[V]
  inline def map3[T, V](p: P[T])(inline f: T => V): P[V] = new P[V]
  def map4[T, V](p: P[T])(f: T => V): P[V] = new P[V]
}

object Test {
  import Ops.*
  val pair: P[(Int, Seq[Int])] = new P
  def a: P[Int] = pair.map1((a, bs) => a + bs.sum)
  def b: P[Int] = pair.map2((a, bs) => a + bs.sum)
  def c: P[Int] = map3(pair)((a, bs) => a + bs.sum)
  def d = map4(pair)((a, bs) => a + bs.sum)
  def e = List((1, "a")).map((n, s) => s * n)
}
