trait Base[+T]
class Impl[+T](val t: T) extends Base[T]
object I:
  def unapply[T](i: Impl[T]): Some[T] = Some(i.t)
class Con[-T]
object C:
  def unapply[T](c: Con[T]): Some[Con[T]] = Some(c)

object Max:
  def a(b: Base[Int]) = b match
    case I(x) => x
    case _ => 0
  def b(b: Any) = b match
    case C(x) => x
    case _ => 0
  def c(l: Seq[Int]) = l match
    case h :: t => h
    case _ => 0

object Main:
  def foo(a: Int | Double) = a match
    case a: (Float | Boolean) => 1
    case _ => 0
  def bar[T](x: T): Int = x match
    case i: Int => i + 1
    case s: String => s.length
    case _ => 0
  def baz(h: Any) = h match
    case 5 :: _ => "hello"
    case List(from) => from
  def qux(x: AnyRef) = x match
    case List(_) => 0
    case _ => -2
