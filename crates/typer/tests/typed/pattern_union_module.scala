sealed trait R[+T, +E]
case class Ok[+T](value: T) extends R[T, Nothing]
case class Err[+E](e: E) extends R[Nothing, E]

object V:
  object Abort
  type Abort = Abort.type
  opaque type Validated[+A] = Ok[A] | Abort
  def mk[A](a: A): Validated[A] = Ok(a)

  def f[T](u: Validated[T]): R[T, Int] = u match
    case ok: Ok[?] => ok
    case _ => Err(1)

  def g[T](u: Ok[T] | Abort): R[T, Int] = u match
    case ok: Ok[?] => ok
    case _ => Err(1)
