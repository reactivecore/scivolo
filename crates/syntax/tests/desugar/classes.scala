case class P[T](x: T, y: Int = 0) derives Eq
object P:
  val zero = 0
class V(val u: Int) extends AnyVal
object O:
  implicit class R(s: String)
