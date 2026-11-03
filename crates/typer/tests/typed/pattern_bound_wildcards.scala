sealed trait PR[+T]
case class Ok[T](value: T) extends PR[T]
case class Bad(msg: String) extends PR[Nothing]

final class FBox[T](val v: T)
object FBox:
  def unapply[T](b: FBox[T]): Some[T] = Some(b.v)

class MD[T, B](val m: Option[String], val name: String, val f: (B, Seq[Any]) => T):
  override def equals(obj: Any): Boolean = obj match
    case x: MD[_, _] => MD.unapply(x) == MD.unapply(this)
    case _ => false
  override def hashCode(): Int = name.hashCode

object MD:
  def unapply[T, B](x: MD[T, B]) = Option((x.m, x.name, x.f))

object Test:
  def f(xs: Seq[Either[String, PR[?]]]): Seq[Any] =
    xs.collect {
      case Left(s) => s
      case Right(Ok(x)) => x
    }

  def g(b: FBox[?]): Any = b match
    case FBox(x) => x
