type Endo[A] = A => A

object Main {
  type Pair[T] = (T, T)
  type Bounded[T <: AnyRef] = List[T]
  type Two[A, B] = Map[A, List[B]]
  type Buf[T] = scala.collection.mutable.ArrayBuffer[T]
  def first[T](p: Pair[T]): T = p._1
  val a: Int = first((1, 2))
  def b(xs: Bounded[String]): Int = xs.length
  def c(m: Two[String, Int]): Int = m.size
  val inc: Endo[Int] = x => x + 1
  def d: Int = inc(2)
  def e(p: Pair[String]): Int = p._2.length
  val buf: Buf[Int] = new scala.collection.mutable.ArrayBuffer[Int]()
}
