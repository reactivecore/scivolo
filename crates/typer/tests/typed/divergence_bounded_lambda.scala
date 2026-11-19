object DivergenceBoundedLambda {
  sealed trait Nat
  sealed trait Succ[Prev <: Nat] extends Nat
  sealed trait Zero extends Nat

  class Sum[M <: Nat, N <: Nat] { type Out <: Nat }

  object Sum {
    type Aux[M <: Nat, N <: Nat, R <: Nat] = Sum[M, N] { type Out = R }

    implicit def sum0[N <: Nat]: Sum.Aux[Zero, N, N] = new Sum[Zero, N] { type Out = N }
    implicit def sum1[M <: Nat, N <: Nat, R <: Nat](implicit sum: Sum.Aux[M, Succ[N], R]): Sum.Aux[Succ[M], N, R] =
      new Sum[Succ[M], N] { type Out = R }
  }

  class Foo[T]
  class Bar[F[_ <: Int]]
  object Bar {
    implicit def barF[F[_ <: Int]](implicit fooF: Foo[Bar[F]]): Bar[F] = null
  }
  class A[T <: Int]
  object A {
    implicit def fooA[F[_[_ <: Int]]](implicit barB: F[B]): Foo[F[A]] = null
  }
  class B[T <: Int]
  object B {
    implicit def fooB[F[_[_ <: Int]]]: Foo[F[B]] = null
  }

  def test = {
    type _3 = Succ[Succ[Succ[Zero]]]
    type _5 = Succ[Succ[_3]]
    implicitly[Sum[_3, _5]]
    implicitly[Bar[A]]
  }
}
