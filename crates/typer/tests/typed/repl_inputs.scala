class Try[+T] {
  def get(default: T => Int): Int = this match {
    case Success(v) => default(v)
    case _ => 0
  }
}
final case class Success[+T](value: T) extends Try[T]

object Main {
  case class Cons[A](h: A, t: List[A])
  case class ::[A](h: A, t: List[A])
  val a = Nil
  val b: scala.collection.immutable.Nil.type = Nil
  val c = Cons(1, Nil).copy(h = 2)
  val d = ::("x", Nil)
  def run(): Int = c.h + d.h.length + Success(3).get(x => x)
}
