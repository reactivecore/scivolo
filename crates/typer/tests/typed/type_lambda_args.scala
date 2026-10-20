object Utils:
  type Id[t] = t
  type Const[c] = [t] =>> c
  type Ap[F[_], T] = F[T]

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
