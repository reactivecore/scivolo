object Parsers:
  inline def charIn(inline s: String*): Int = s.length
  inline def h1(x: Int*): Int = x.sum
  inline def h2(inline x: Int*): Int = x.sum
  inline def i1(x: => Int*): Int = x.sum
  inline def i2(inline x: => Int*): Int = x.sum
  inline def first(inline xs: Int*): Int = xs.head

def y: Int = 43

def use =
  val a = Parsers.charIn("0-9")
  val b = Parsers.charIn("a-z", "A-Z")
  val c = Parsers.h1(42)
  val d = Parsers.h2(y, y)
  val e = Parsers.i1(y)
  val f = Parsers.i2(y, y)
  val g = Parsers.h2(Seq(1, 2)*)
  val h = Parsers.charIn()
  val i = Parsers.first(1, 2)
  a + b + c + d + e + f + g + h + i
