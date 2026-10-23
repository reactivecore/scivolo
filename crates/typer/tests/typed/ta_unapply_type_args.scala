object Poly:
  def unapply[T](x: T): Option[String] = Some(x.toString)

class Mono[T]:
  def unapply(x: Int): Option[Int] = Some(x + 1)

object Mono

object Test:
  def poly(x: Int): String = x match
    case Poly[Int](s) => s

  def mono(x: Int): Int = x match
    case Mono[String](y) => y
