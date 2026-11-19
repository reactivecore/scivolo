class A:
  def super$x: Int = 1
  def y: Int = 2
object O:
  val a = A()
  export a.*
  def z = super$x + y
