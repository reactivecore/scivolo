object GadtFboundedApprox {
  sealed trait Sub[-X, +Y]
  final case class Refl[Z]() extends Sub[Z, Z]

  def f[T <: Comparable[T]](s: Sub[T, String], t: T): Unit = {
    var v = t
    s match {
      case _: Refl[z] =>
        val x: String = v
        ()
    }
  }
}
