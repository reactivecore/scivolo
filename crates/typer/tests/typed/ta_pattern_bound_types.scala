enum Opt[+T]:
  case Some[T](x: T) extends Opt[T]
  case None

enum Fun[-T, +U >: Null]:
  case Identity[T, U >: Null](g: T => U) extends Fun[T, U]
  case Const

  def f: T => U = this match
    case Identity(g) => g
    case Const => _ => null

case class X2[A, B](a: A, b: B)

object Test:
  def get(o: Opt[String]): String = o match
    case Opt.Some(x) => x
    case Opt.None => ""

  def long(x: Any): Long = x match
    case Array(i: Long) => i
    case _ => 0L

  def first(x: Any): Any =
    val X2(a, b) = x: @unchecked
    a
