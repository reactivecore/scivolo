object LaterClauses {
  def f(x: Int)(y: Int) = y
  def f(x: Int)(y: String) = y.length
  def g[T](x: T)(y: Int) = y
  def g[T](x: T)(y: String) = y.length
  def h[T](x: T)(y: T)(z: Int) = z
  def h[T](x: T)(y: T)(z: String) = z.length

  extension (x: Char)
    def e(p: String): String = "a"
    def e(min: Boolean, max: Int): String = "b"

  trait A
  trait B
  class C extends A with B
  def fr(x: A): A = x
  def fr(x: B): B = x
  def ab(x: Byte): Array[Byte] = Array(x)
  def ab(x: Int): Array[Int] = Array(x)

  trait Rp[T, V]
  object Rp { implicit def rp[T]: Rp[T, Seq[T]] = new Rp[T, Seq[T]] {} }
  class Px[+T]
  extension [T](p: Px[T]) def rep[V](using r: Rp[T, V]): Px[V] = new Px[V]
  extension [T](p: => Px[T]) def rep[V](min: Int)(using r: Rp[T, V]): Px[V] = new Px[V]
  def u(using x: Rp[Int, Seq[Int]]): Int = 1
  def u(s: String): Int = 2
  extension [T](p: Px[T]) def rep2[V, W <: Seq[V]](using r: Rp[T, W]): Px[W] = new Px[W]
  extension [T](p: => Px[T]) def rep2[V](min: Int): Px[V] = new Px[V]
  def v(using x: Rp[Int, Seq[Int]])(i: Int): Int = i
  def v(s: String): Int = 2

  def test(): Unit = {
    f(1)(2)
    f(1)("two")
    g[Int](1)(2)
    g(1)("two")
    h(1)(1)(2)
    h(1)(1)("two")
    e('c')(false, 1)
    'c'.e("a")
    val a: A = fr(new C)
    val b: B = fr(new C)
    val bytes: Array[Byte] = ab(1)
    val reps: Px[Seq[Int]] = new Px[Int].rep(1)
    val us = u("s")
    val reps2: Px[Int] = new Px[Int].rep2(1)
    val vs = v(1) + v(using Rp.rp[Int])(2)
  }
}
