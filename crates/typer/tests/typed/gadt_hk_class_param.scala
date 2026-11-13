sealed trait EQK[F[_], G[_]]
final case class ReflK[F[_]]() extends EQK[F, F]

sealed trait Expr[-F[_]]
final case class ConstExpr() extends Expr[[X] =>> Int]

class Holder[F[_]] {
  def same[G[_], A](fa: F[A], eqk: EQK[F, G]): G[A] = eqk match {
    case ReflK() => fa
  }
}

class GadtHkClassParam {
  def m1[F[_], G[_], A](fa: F[A], eqk: EQK[F, G]): G[Int] = eqk match {
    case ReflK() =>
      val r: G[A] = fa
      ???
  }

  def bounds[F[_], A](e: Expr[F]): Int = e match {
    case _: ConstExpr =>
      val i: Int = ??? : F[A]
      i
  }
}
