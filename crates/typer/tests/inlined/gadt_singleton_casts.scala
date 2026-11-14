class GadtSingletonCasts {
  class Exactly[T]
  def e[T]: Exactly[T] = ???

  inline def g1[T](t: T): Int =
    e[T] match {
      case _: Exactly[Int] => t
      case _ => 0
    }

  inline def g3[T](t: T): Int =
    e[T] match {
      case _: Exactly[Int] => val i: Int = t; i
      case _ => 0
    }

  inline def lit[T](t: T): T =
    e[T] match {
      case _: Exactly[Int] => 23
      case _ => t
    }

  def a1 = g1[Int](5)
  def a3 = g3[Int](5)
  def b1(y: Int) = g1[Int](y)
  def c = lit[Int](5)
  def d = lit[String]("s")
}
