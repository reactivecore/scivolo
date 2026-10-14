object Y
class Box[T <: CharSequence](val value: T)
class Ord[T <: Comparable[T]](val value: T)
class C:
  def same(x: Any) = x match
    case _: this.type => "this"
    case _ => "other"

object Main:
  type IS = Int | String
  def w(x: Any) = x match
    case xs: List[?] => xs.size
    case m: Map[_, _] => m.size
    case q: Either[Option[_], ?] => q.isLeft.hashCode
    case b: Box[?] => b.value.length
    case o: Ord[?] => o.value.hashCode
    case _ => 0
  def s(x: Any) = x match
    case _: Y.type => 1
    case y: "a" => y.length
    case _: 3 => 3
    case _ => 0
  def p(a: AnyRef, x: Any) = x match
    case _: a.type => true
    case _ => false
  def u(x: Any) = x match
    case v: IS => v.toString
    case _ => ""
