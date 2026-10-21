trait Box[+A]
trait Tc[A]
object Tc {
  implicit def boxTc[A](implicit t: => Tc[Box[A]]): Tc[Box[A]] = new Tc[Box[A]] {}
}

trait Grow[A]
object Grow {
  implicit def grow[A](implicit t: => Grow[Box[Box[A]]]): Grow[Box[A]] = new Grow[Box[A]] {}
  implicit def base[A]: Grow[A] = new Grow[A] {}
}

trait Ord[T]
object Ord {
  implicit def ordered[T <: Comparable[T]]: Ord[T] = new Ord[T] {}
}

object ImplicitWildcards {
  def need[A >: Int](implicit t: Tc[Box[A]]): A = ???
  def knot = need
  def noKnot = implicitly[Grow[Box[Int]]]
  def sorted[T <: Comparable[T]](implicit o: Ord[T]): T = ???
  def fBounded: String = sorted
}
