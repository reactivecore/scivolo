object Implicits {
  trait Sequencer[-T, V, R]
  object Sequencer extends LowPriSequencer {
    implicit def SingleSequencer[T]: Sequencer[Unit, T, T] = null
  }
  trait LowPriSequencer extends LowestPriSequencer[Sequencer] {
    implicit def UnitSequencer[T]: Sequencer[T, Unit, T] = null
  }
  trait LowestPriSequencer[S[_, _, _]] {
    implicit def Sequencer1[T1, T2]: S[T1, T2, (T1, T2)] = null.asInstanceOf[S[T1, T2, (T1, T2)]]
  }
  trait Repeater[-T, R]
  object Repeater extends LowPriRepeater {
    implicit object UnitRepeater extends Repeater[Unit, Unit]
  }
  trait LowPriRepeater {
    implicit def GenericRepeaterImplicit[T]: Repeater[T, Seq[T]] = null
  }
}

class Concat[-A, +C]
trait Rev[-A]
object Rev {
  implicit val rev: Rev[CharSequence] = null
}
trait LowConcat {
  implicit def default[A]: Concat[A, Any] = null
}
object Concat extends LowConcat {
  implicit def reversible[A](implicit r: Rev[A]): Concat[A, String] = null
}

class P[+T]

object Ops {
  import Implicits.*
  extension [T](p: P[T]) {
    def ~[V, R](other: P[V])(using s: Sequencer[T, V, R]): P[R] = new P[R]
    def rep[V](using r: Repeater[T, V]): P[V] = new P[V]
  }
}

object Test {
  import Ops.*
  val unit: P[Unit] = new P
  val int: P[Int] = new P
  def a = unit ~ unit
  def b = unit ~ int
  def c = int ~ unit
  def d = int ~ int
  def e = int ~ int ~ int
  def f = unit.rep
  def g = int.rep
  def h = (unit ~ unit).rep
  def cat[A, C](a: A)(implicit c: Concat[A, C]): C = ???
  def i = cat("s")
}
