//> using scala 3.9.0
class P(val x: Int):
  override def toString(): String = "P(" + x + ")"

object Cat:
  def two(a: String, b: String): String = a + b
  def chain(a: String, i: Int, c: Char, d: Double): String = a + ": " + i + c + d
  def literals(s: String): String = "x" + 12 + s + true + 2.5 + 'c' + 7L
  def empty(i: Int): String = "" + i
  def emptyMiddle(a: String, b: String): String = a + "" + b
  def small(b: Byte, s: Short): String = "b" + b + s
  def objects(p: P, q: Any): String = "p=" + p + ", q=" + q
  def nulls(s: String): String = s + null
  def sequence(a: String, cs: CharSequence): String = a + cs

@main def strings(): Unit =
  println(Cat.two("a", "b"))
  println(Cat.chain("n", 3, 'z', 1.5))
  println(Cat.literals("-"))
  println(Cat.empty(42))
  println(Cat.emptyMiddle("l", "r"))
  println(Cat.small(1.toByte, 2.toShort))
  println(Cat.objects(new P(4), "q"))
  println(Cat.nulls("s"))
  println(Cat.sequence("a", "cs"))
