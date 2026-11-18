package glb

object Nest {
  class A
  class Z extends A
  def glb(x: Z & (A | Null)): Int = 0
}

class Ab { class X }
class Ab$c
class V1(val x: Int) extends AnyVal
class V2(val x: String) extends AnyVal

object FullNames {
  def named(a: Ab)(x: a.X & Ab$c): Int = 0
  def swapped(a: Ab)(x: Ab$c & a.X): Int = 0
  def values(x: V2 & V1): Int = 0
}

object Operators {
  def <(x: Int): Int = x
  def +(x: Int): Int = x
  def a(x: Int): Int = x
}
