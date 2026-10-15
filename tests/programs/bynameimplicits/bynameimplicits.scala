trait Link { def next: Link }
object Link {
  implicit def link(implicit rec: => Link): Link = new Link { def next = rec }
}

trait Nat
case class S(x: Nat) extends Nat
case class Z() extends Nat

trait Sum[+A, +B]
case class Fst[+A](x: A) extends Sum[A, Nothing]
case class Snd[+B](x: B) extends Sum[Nothing, B]

trait shaped[A, B] { def toShape(x: A): B }

trait Countable[T] { def count(x: T): Int }

object bynameimplicits {
  implicit def natShape: Nat `shaped` Sum[Nat, Z] = new (Nat `shaped` Sum[Nat, Z]) {
    def toShape(n: Nat) = n match {
      case S(m) => Fst(m)
      case Z() => Snd(Z())
    }
  }

  implicit def shapedCountable[T, U](implicit ev1: T shaped U, ev2: Countable[U]): Countable[T] =
    new Countable[T] { def count(x: T) = ev2.count(ev1.toShape(x)) }

  implicit def sumCountable[T, U](implicit ev1: => Countable[T]): Countable[Sum[T, U]] =
    new Countable[Sum[T, U]] {
      def count(s: Sum[T, U]) = s match {
        case Fst(x) => ev1.count(x) + 1
        case Snd(_) => 0
      }
    }

  def count(x: Nat) = implicitly[Countable[Nat]].count(x)

  def main(args: Array[String]): Unit = {
    val l = implicitly[Link]
    println(l eq l.next)
    println(l.next eq l.next.next)
    println(count(S(S(S(Z())))))
  }
}
