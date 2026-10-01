//> using scala 3.9.0
object Ops:
  def add(a: Int, b: Int): Int = a + b
  def mix(a: Long, b: Int): Long = a * b - 1L
  def ratio(a: Double, b: Double): Double = a / b
  def less(a: Int, b: Int): Boolean = a < b
  def same(a: Int, b: Int): Boolean = a == b
  def neg(a: Int): Int = -a
  def not(b: Boolean): Boolean = !b
  def both(a: Boolean, b: Boolean): Boolean = a && b
  def either(a: Boolean, b: Boolean): Boolean = a || b
  def bits(a: Int): Int = (a << 2) | 1
  def rem(a: Int, b: Int): Int = a % b
  def char(c: Char): Int = c + 1

@main def arith(): Unit =
  println(Ops.add(2, 3))
  println(Ops.mix(7L, 6))
  println(Ops.ratio(1.0, 4.0))
  println(Ops.less(1, 2))
  println(Ops.same(3, 3))
  println(Ops.neg(4))
  println(Ops.not(true))
  println(Ops.both(true, false))
  println(Ops.either(false, true))
  println(Ops.bits(3))
  println(Ops.rem(7, 3))
  println(Ops.char('a'))
