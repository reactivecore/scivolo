object M {
  var n = 0
  class Exactly[T]
  def exact[T]: Exactly[T] = { n += 1; new Exactly[T] }
  inline def exactType[T]: Exactly[T] = exact[T]
  inline def foo[T](t: T): T =
    inline exactType[T] match {
      case _: Exactly[Int] => 23
      case _ => t
    }
  inline def bar[T](t: T): T =
    inline exact[T] match {
      case _: Exactly[Int] => 23
      case _ => t
    }
  def a = foo(1)
  def b = bar("b")
}
