package glb

object Nest {
  class A
  class Z extends A
  def glb(x: Z & (A | Null)): Int = 0
}

object Operators {
  def <(x: Int): Int = x
  def +(x: Int): Int = x
  def a(x: Int): Int = x
}
