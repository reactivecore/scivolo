object GadtInlineBlockCast {
  class Exactly[T]
  def e[T]: Exactly[T] = ???

  def plain[T](t: T): T =
    e[T] match {
      case _: Exactly[Int] => 23
      case _ => t
    }

  inline def inlined[T](t: T): T =
    e[T] match {
      case _: Exactly[Int] => 23
      case _ => t
    }

  inline def inlineMatch[T](t: T): T =
    inline e[T] match {
      case _: Exactly[Int] => 23
      case _ => t
    }

  def local[T](t: T): Int =
    inline def g(x: T): T =
      e[T] match {
        case _: Exactly[Int] => 23
        case _ => x
      }
    1
}
