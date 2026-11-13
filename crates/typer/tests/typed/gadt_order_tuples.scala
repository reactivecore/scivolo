object GadtOrderTuples {
  sealed trait EQ[A, B]
  final case class Refl[A]() extends EQ[A, A]

  def tuples[A, B, C, D](xx: Any, eq: EQ[(A, B), (C, D)]): Unit =
    eq match {
      case _: Refl[x] =>
        val p1: (C, D) = xx.asInstanceOf[x]
        val p2: (A, B) = xx.asInstanceOf[x]
        val p3: x = xx.asInstanceOf[(C, D)]
        val p4: x = xx.asInstanceOf[(A, B)]
        val q1: A = xx.asInstanceOf[C]
        val q2: C = xx.asInstanceOf[A]
        ()
    }

  def options[A, C](xx: Any, eq: EQ[Option[A], Option[C]]): Unit =
    eq match {
      case _: Refl[x] =>
        val p1: Option[C] = xx.asInstanceOf[x]
        val p2: Option[A] = xx.asInstanceOf[x]
        val p3: x = xx.asInstanceOf[Option[C]]
        val p4: x = xx.asInstanceOf[Option[A]]
        val q1: A = xx.asInstanceOf[C]
        val q2: C = xx.asInstanceOf[A]
        ()
    }
}
