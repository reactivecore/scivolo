class Foo[A, +B](val a: A, val b: B)
object Foo {
  def unapply[X <: Int, Y >: X <: Int](f: Foo[Y, X]): Some[Int] = Some(f.b)
}

class GadtMaximizeOrder {
  def f[T](x: Foo[? <: T, Int]): Unit = x match {
    case Foo(n) => ()
  }
}
