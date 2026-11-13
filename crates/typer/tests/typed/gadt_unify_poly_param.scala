sealed trait Op[A, B]
final case class Dup[T](x: T) extends Op[T, (T, T)]

class GadtUnifyPolyParam {
  def foo[R](f: [A, B] => (o: Op[A, B]) => R): R = ???

  def m1: Unit =
    foo([A, B] => (o: Op[A, B]) =>
      o match {
        case o: Dup[t] =>
          var a1: t = o.x
          var a2: A = o.x
          a1 = a2
          a2 = a1
          var b1: (t, t) = (o.x, o.x)
          var b2: B = (o.x, o.x)
          b1 = b2
          b2 = b1
          ()
      }
    )
}
