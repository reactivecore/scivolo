//> using scala 3.9.0
object S:
  def boxed(i: Int): Integer = Integer.valueOf(i)
  def larger(a: Int, b: Int): Int = Math.max(a, b)
  def parsed(s: String): Int = Integer.parseInt(s)
  def digit(c: Char): Boolean = Character.isDigit(c)
  def text(l: Long): String = java.lang.Long.toString(l)
  def most(): Int = Integer.MAX_VALUE
  def scalaConst(): Int = K.k
  def out(s: String): Unit = System.out.println(s)

object K:
  final val k = 3

@main def statics(): Unit =
  println(S.boxed(3))
  println(S.larger(2, 7))
  println(S.parsed("42"))
  println(S.digit('5'))
  println(S.text(9L))
  println(S.most())
  S.out("out")
