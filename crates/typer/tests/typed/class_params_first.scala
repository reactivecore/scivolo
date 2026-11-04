trait SeqLike[+Repr]
trait Seq extends SeqLike[Seq]
trait MySeq extends Seq with SeqLike[MySub]
trait MySub extends MySeq

class X[T] { type Id }
object A extends X[B[D]]
class B[C](id: A.Id)
class D

class Sams {
  import O.*
  def app[T, U](x: T)(f: F[T, U]): U = f(x)
  app(1)(x => List(x))
}
object O {
  trait F[T, U] { def apply(x: T): U }
}
