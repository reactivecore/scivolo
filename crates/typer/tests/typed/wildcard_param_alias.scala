class Box[+A <: AnyRef]:
  def isEmpty = false
  def get: A = ???

object G:
  def unapply(m: Any): Option[?] = None

object B:
  def unapply(m: Any): Box[?] = Box[String]

object S:
  def unapply(m: Any): Box[? <: String] = Box[String]

object Test:
  def f(x: Any) = x match
    case G(v) => v
    case B(v) => v
    case S(v) => v
