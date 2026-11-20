package object shapes {
  type Name = String
  def shout(s: Name): Name = s.toUpperCase
}

package shapes {

trait Functor[F[_]] {
  def map[A, B](fa: F[A])(f: A => B): F[B]
}

object Functor {
  implicit val listFunctor: Functor[List] = new Functor[List] {
    def map[A, B](fa: List[A])(f: A => B): List[B] = fa.map(f)
  }
  def lift[F[_], A, B](f: A => B)(implicit F: Functor[F]): F[A] => F[B] = fa => F.map(fa)(f)
  def either[E]: Functor[({ type L[A] = Either[E, A] })#L] = new Functor[({ type L[A] = Either[E, A] })#L] {
    def map[A, B](fa: Either[E, A])(f: A => B): Either[E, B] = fa.map(f)
  }
}

sealed trait Shape { def area: Double }
case class Rect(w: Double, h: Double) extends Shape { def area: Double = w * h }
case object Dot extends Shape { def area: Double = 0.0 }
case class Pt(x: Int, y: Int = 7)

class Tools {
  type Pair[A] = (A, A)
  def swap[A](p: Pair[A]): Pair[A] = (p._2, p._1)
  def greet(name: String = "you", punct: String = "!"): String = "hi " + name + punct
  def total(xs: Int*): Int = xs.sum
  def twice(x: => Int): Int = x + x
  def sizes(m: Map[_, _]): Int = m.size
  def longest(xs: List[_ <: CharSequence]): Int = xs.map(_.length).max
  def both(a: Comparable[String] with java.io.Serializable): Boolean = a.compareTo("") > 0
  def ordered[A <: Ordered[A]](a: A, b: A): A = if (a < b) a else b
  @deprecated("use greet", "1.0") def hello: String = "hello"
  def checked: Int @unchecked = 3
}

object Tools {
  type Id[A] = A
  def id[A](a: Id[A]): Id[A] = a
  object Nested {
    val answer: Int = 42
    class Counter(start: Int) { def next: Int = start + 1 }
  }
  implicit class RichInt(val n: Int) {
    def squared: Int = n * n
  }
  private[shapes] def secret: Int = 7
}

trait Container {
  type Elem
  def elems: List[Elem]
  def first: Elem = elems.head
}

object Strings extends Container {
  type Elem = String
  def elems: List[String] = List("a", "b")
}

}
