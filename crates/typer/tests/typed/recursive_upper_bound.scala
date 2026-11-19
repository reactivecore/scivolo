object RecursiveUpperBound {
  trait Comp[T]
  trait Coll[T]
  trait Or[A]
  trait Contra[-A]
  trait Cov[+B]

  def max[M <: Comp[? >: M]](x: Coll[? <: M]): M = ???
  def rec1[T <: Cov[T]]: Contra[T] = ???
  def ipl[A](implicit from: A => Or[A]): Int = 0

  val m = max(null)
  val c = rec1
  val i = ipl
}
