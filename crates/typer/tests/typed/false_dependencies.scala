object Main:
  class C[T] { type TT = T }
  def foo[U](x: Int)(y: C[U])(z: y.TT): U = z
  val s: String = foo(3)(new C[String])("hello")
  def bar[U](y: C[U])(z: y.TT): List[U] = List(z)
  val l = bar(new C[Int])(1)
