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
  }
}
