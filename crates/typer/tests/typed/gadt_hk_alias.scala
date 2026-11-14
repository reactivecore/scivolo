object GadtHkAlias {
  sealed trait L[E[_, _], R]
  case class Ap[E[_, _], R, R_](f: L[E, E[R, R_]]) extends L[E, R_]

  def same[A](a: A, b: A): A = a
  def inst[E[_, _], X](l: L[E, X]): X = ???

  def m[Exp[_, _], T](exp: L[Exp, T]): Unit = exp match {
    case app: Ap[e, r, r_] =>
      val f1 = app.f
      val g = same(f1, (??? : L[Exp, Exp[r, r_]]))
      val a: L[Exp, Exp[r, r_]] = f1
      val b: L[e, e[r, r_]] = a
      val c: Exp[r, r_] = (??? : e[r, r_])
      val d: e[r, r_] = (??? : Exp[r, r_])
      val x = inst(app.f)
      ()
  }
}
