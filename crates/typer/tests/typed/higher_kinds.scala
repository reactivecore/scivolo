trait Ops:
  type F[T]
  def wrap[T](t: T): F[T]
  def un[T](f: F[T]): T

object L extends Ops:
  type F[T] = List[T]
  def wrap[T](t: T): List[T] = List(t)
  def un[T](f: List[T]) = f.head

trait Functor[F[_]]:
  def map[A, B](fa: F[A])(f: A => B): F[B]

object ListFunctor extends Functor[List]:
  def map[A, B](fa: List[A])(f: A => B) = fa.map(f)

object Main:
  def first[CC[X] <: Seq[X], A](xs: CC[A]): A = xs.head
  val a: Int = L.un(L.wrap(1))
  val b: List[Int] = ListFunctor.map(List(1))(_ + 1)
  val c: Int = first(List(1, 2))
