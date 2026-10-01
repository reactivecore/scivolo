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
  def bytes(a: Byte, b: Byte): Int = a + b
  def longFloat(a: Long, b: Float): Float = a + b
  def chars(a: Char, b: Char): Int = a - b
  def narrow(d: Double): Int = d.toInt
  def toChar(i: Int): Char = i.toChar
  def toByte(i: Int): Byte = i.toByte
  def invLong(a: Long): Long = ~a
  def invChar(c: Char): Int = ~c
  def negFloat(f: Float): Float = -f
  def mixedEq(a: Int, b: Long): Boolean = a == b
  def boolNe(a: Boolean, b: Boolean): Boolean = a != b
  def ushr(a: Long): Long = a >>> 3
  def charLess(c: Char, i: Int): Boolean = c < i
  def fmod(a: Double, b: Double): Double = a % b
  def negDiv(a: Int, b: Int): Int = a / b

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
  println(Ops.bytes(Ops.toByte(1), Ops.toByte(2)))
  println(Ops.longFloat(3L, 0.5f))
  println(Ops.chars('c', 'a'))
  println(Ops.narrow(-2.7))
  println(Ops.toChar(66))
  println(Ops.toByte(300))
  println(Ops.invLong(5L))
  println(Ops.invChar('a'))
  println(Ops.negFloat(1.5f))
  println(Ops.mixedEq(3, 3L))
  println(Ops.boolNe(true, false))
  println(Ops.ushr(-16L))
  println(Ops.charLess('a', 100))
  println(Ops.fmod(7.5, 2.0))
  println(Ops.negDiv(-7, 2))
