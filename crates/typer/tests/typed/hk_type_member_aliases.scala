object O:
  opaque type F[A] = List[A]
  def mk[A](a: A): F[A] = List(a)

type X = O.F

trait T:
  type F[A]
  type Y = F
  def mk(i: Int): Y[Int]
  def get(x: F[Int]): Int

object Impl extends T:
  type F[A] = Option[A]
  def mk(i: Int): Y[Int] = Some(i)
  def get(x: F[Int]): Int = x.get

def twice[G[_]](g: G[Int]): List[G[Int]] =
  type Z = G
  val z: Z[Int] = g
  List(z, z)

def test =
  val x: X[Int] = O.mk(1)
  (x, Impl.get(Impl.mk(3)), twice(Option(1)))
