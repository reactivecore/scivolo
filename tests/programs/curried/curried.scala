//> using scala 3.9.0
class Box(val v: Int, val w: Int):
  def add(a: Int)(b: Int): Int = v + w + a + b

object Cur:
  def three(a: Int)(b: String)(c: Boolean): String = b + a + c
  def empty()(x: Int): Int = x
  def rest(a: Int)(xs: Int*): Int = a + xs.length
  def call(): String = three(1)("b")(true)

@main def curried(): Unit =
  println(Cur.call())
  println(Cur.three(2)("x")(false))
  println(Cur.empty()(5))
  println(Cur.rest(10)(1, 2))
  val b = new Box(1, 2)
  println(b.add(3)(4))
  println(b.w)
