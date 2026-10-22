object Ext {
  extension [T](xs: List[T]) def second: T = xs.tail.head
  extension (x: Int) {
    def double: Int = x * 2
    def +++(y: Int): Int = x + y
  }
  extension (s: String) def times(n: Int)(using sep: String): String = List.fill(n)(s).mkString(sep)
  extension (l: Long) def half: Long = l / 2
  extension [T](p: => List[T]) def rep[V](min: Int = 0, max: Int = 9): List[V] = Nil
  extension [T](inline p: List[T])
    inline def !(using ctx: Ctx): Int = ctx.index + p.size
    inline def many[V](using ctx: Ctx): List[V] = Nil
}

extension (b: Boolean) def toInt: Int = if b then 1 else 0

class Ctx(var index: Int)

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
  def i: List[String] = List(1).rep(1)
  def j: List[String] = List(2).rep(max = 3)
  given Ctx = Ctx(1)
  def k: Int = List(1, 2).!
  def l: Int = List(1).!.abs
  def m: List[Int] = List(1).many
}
