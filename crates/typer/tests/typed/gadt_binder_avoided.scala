enum Var[G, A] {
  case Z[G, A]() extends Var[(A, G), A]
  case S[G, A, B](x: Var[G, A]) extends Var[(B, G), A]
}

class GadtBinderAvoided {
  import Var.*

  def evalVar[G, A](x: Var[G, A])(rho: G): A = x match {
    case _: Z[g, a] =>
      rho(0)
    case s: S[g, a, b] =>
      evalVar(s.x)(rho(1))
  }

  def first[G, A](x: Var[G, A])(rho: G): A = x match {
    case _: Z[g, a] =>
      val y: a = rho(0)
      y
    case _ => ???
  }
}
