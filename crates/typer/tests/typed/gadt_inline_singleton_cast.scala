class GadtInlineSingletonCast {
  class Exactly[T]
  def e[T]: Exactly[T] = ???

  transparent inline def g[T](t: T): Int =
    e[T] match {
      case _: Exactly[Int] => t
      case _ => 0
    }

  def literal = g[Int](5)
  def path(y: Int) = g[Int](y)
}
