//> using scala 3.9.0
// Local inline methods, also using the enclosing class, and local methods with interleaved type
// parameter clauses.

class Counter(val base: Int) {
  private var n = 0
  def run(): Int = {
    inline def bump(by: Int): Int = { n += by; n + base }
    val f = (x: Int) => bump(x)
    bump(1) + f(2)
  }
}

object localinline {
  def outer(y: Int): Int = {
    inline def inc(inline z: Int): Int = z + y
    inline def inc2(z: Int): Int = inc(inc(z))
    inc2(1)
  }

  def main(args: Array[String]): Unit = {
    inline def size[T <: Tuple](t: T): Int = compiletime.constValue[Tuple.Size[T]]
    println(size((1, "a", 2.0)))
    println(new Counter(5).run())
    println(outer(10))
    extension [T](xs: List[T]) def mapSum[U](f: T => U)(using n: Numeric[U]): U = xs.map(f).sum
    println(List(1, 2, 3).mapSum(_ * 2))
    def pair[A](a: A)[B](b: B): (A, B) = (a, b)
    println(pair(1)("x"))
  }
}
