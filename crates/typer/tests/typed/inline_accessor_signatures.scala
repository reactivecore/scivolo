package p

import scala.quoted.*

private[p] inline def allReaders[T, R[_]]: Int = ${ allReadersImpl[T, R] }
private def allReadersImpl[T, R[_]](using Quotes, Type[T], Type[R]): Expr[Int] = ???

object O:
  inline def twice[A](a: A): List[A] = mk[A](a)(using 2)
  private def mk[A](a: A)(using n: Int): List[A] = List.fill(n)(a)
  inline def pairs[A <: AnyVal, F[_]](f: F[A]): Option[F[A]] = wrap[A, F](f)
  private def wrap[A <: AnyVal, F[_]](f: F[A]): Option[F[A]] = Some(f)
