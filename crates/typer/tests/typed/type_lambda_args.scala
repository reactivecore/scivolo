object Utils:
  type Id[t] = t
  type Const[c] = [t] =>> c
  type Ap[F[_], T] = F[T]
  type Cov[+T] = Option[T]
  opaque type Pred[A] = A => Boolean
  opaque type Pair[H, T] <: (Int, Int) = (Int, Int)
  def pred[A](a: A): Pred[A] = _ == a
  def test[A](p: Pred[A], a: A): Boolean = p(a)
  def pair[H, T]: Pair[H, T] = (1, 2)
  val contra: Contra[Pred] = new Contra[Pred]:
    def contramap[A, B](fa: Pred[A])(f: B => A): Pred[B] = b => fa(f(b))

trait Contra[F[_]]:
  def contramap[A, B](fa: F[A])(f: B => A): F[B]

import Utils.*

class Functor[F[_]](val kind: String)

object Functor:
  implicit val functorId: Functor[Id] = new Functor[Id]("id")
  implicit def functorConst[T]: Functor[Const[T]] = new Functor[Const[T]]("const")
  given tuple1: Functor[[T] =>> Tuple1[T]] = new Functor("tuple1")

object Main:
  val k: Functor[Const[Int]] = Functor.functorConst[Int]
  val t: Functor[[T] =>> Tuple1[T]] = Functor.tuple1
  val a: Ap[List, Int] = List(1)
  def widen(c: Cov[Int]): Cov[Any] = c
  def widenF[F[+_]](f: F[Int]): F[Any] = f
  val p: Boolean = test(pred(1), 1)
  val first: Int = pair[String, Int]._1
