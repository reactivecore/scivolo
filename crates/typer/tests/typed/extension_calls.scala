object Ext {
  extension [T](xs: List[T]) def second: T = xs.tail.head
  extension (x: Int) {
    def double: Int = x * 2
    def +++(y: Int): Int = x + y
  }
  extension (s: String) def times(n: Int)(using sep: String): String = List.fill(n)(s).mkString(sep)
  extension (l: Long) def half: Long = l / 2
}

extension (b: Boolean) def toInt: Int = if b then 1 else 0

object Main {
  import Ext.*
  given String = ","
  def a: Int = List(1, 2, 3).second
  def b: Int = 3.double
  def c: Int = 1 +++ 2
  def d: String = "x".times(3)
  def e: Int = true.toInt
  def f: Long = 10L.half
  def g: Int = List("a").second.length
  def h: Int = 2.double.double
}
