//> using scala 3.9.0
object I:
  def esc(n: Int): String = s"a\tb\nA$n$$"
  def raw1(n: Int): String = raw"a\tb$n"
  def plain(): String = s"plain"
  def ranges(): String = 1.until(3).toString() + 1.until(10, 2).toString()
  def ops(s: String): String = s.take(2) + s.capitalize + s.stripPrefix("a")
  def lazyArg(o: Option[Int]): Int = o.getOrElse({ println("evaluated"); 2 })

@main def interp(): Unit =
  println(I.esc(1))
  println(I.raw1(2))
  println(I.plain())
  println(I.ranges())
  println(I.ops("abc"))
  println(I.lazyArg(Some(5)))
  println(I.lazyArg(None))
