object Impl:
  val base: Int = 40
  def add(n: Int, step: Int = 1): Int = base + n * step
  def sum[T](xs: T*)(f: T => Int): Int = xs.map(f).sum
  type Name = String
  type Pair[A] = (A, A)
  class Box[+A](val a: A)
  object Box
  trait Shape:
    def area: Int
  given Int = 3
  extension (s: String) def shout: String = s.toUpperCase

export Impl.{add, base, sum, Name, Pair, Box, Shape, shout, given}

object Use:
  def twice: Int = add(1) * 2
  val square: Shape = new Shape { def area = 4 }

@main def topexports(): Unit =
  println(add(1))
  println(add(1, 2) + base)
  println(sum(1, 2, 3)(_ * 2))
  val n: Name = "name"
  println(n)
  val p: Pair[Int] = (1, 2)
  println(p)
  val b: Box[Any] = new Impl.Box(5)
  println(b.a)
  println(summon[Int])
  println("hi".shout)
  println(Use.twice + Use.square.area)
