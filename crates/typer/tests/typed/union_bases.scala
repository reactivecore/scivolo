trait Two[A, +B] { def a: A; def b: B }
trait Cov[+A] { def get: A }

object UnionBases {
  def h(x: Two[Int, String] | Two[Int, Int]) = (x.a + 1, x.b)
  def u(x: Cov[Int] | Cov[String]) = x.get
}
