package glb

object Nest {
  class A
  class Z extends A
  def glb(x: Z & (A | Null)): Int = 0
}
