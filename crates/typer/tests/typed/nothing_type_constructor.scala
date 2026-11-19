class Box[F[_]](val v: F[Int])

object P:
  def f[F[_]](x: F[Int]): F[Int] = x
  def g: Nothing = f[Nothing](???)
  def b: Box[Nothing] = new Box[Nothing](???)
  def h = b.v
  type N = Nothing
  def k: Box[N] = new Box[N](???)
  def n: Nothing = k.v
