class GadtInlineRetyped {
  class Exactly[T]
  def e[T]: Exactly[T] = ???

  inline def nested[T](t: T): T =
    e[T] match {
      case _: Exactly[Int] => { val a = 1; { val b = 2; 23 } }
      case _ => t
    }

  inline def tried[T](t: T): T =
    e[T] match {
      case _: Exactly[Int] => try { println(); 23 } finally println()
      case _ => t
    }

  inline def inner[T](t: T): T =
    e[T] match {
      case _: Exactly[Int] => e[T] match { case _: Exactly[Int] => { println(); 23 } }
      case _ => t
    }
}
