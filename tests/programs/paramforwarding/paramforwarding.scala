//> using scala 3.9.0
abstract class Base(val x: Int, _y: Int) { def y: Int = _y }
class Derived(x: Int, y: Int) extends Base(x, y) { def sum: Int = x + y }
class Unused(x: Int) extends Base(x, 0) { def one: Int = 1 }
class Chain(x: Int) extends Derived(x, 1) { def twice: Int = x * 2 }
class Renamed(z: Int) extends Base(z, 2) { def get: Int = z }
class Hidden(private val x: Int) { def hidden: Int = x }
class OverHidden(x: Int) extends Hidden(x) { def get: Int = x }
class Generic[T](val t: T)
class Concrete(t: String) extends Generic[String](t) { def len: Int = t.length }
class Nested(x: Int) extends Base(x, 3) {
  class Inner { def outerX: Int = x + 1 }
  def inner: Inner = new Inner
}

object Lifted {
  class Ops[A](val self: List[A]) extends AnyVal {
    def foldLeft[B](z: B)(f: (B, A) => B): B = self.foldLeft(z)(f)
    def count(f: A => Boolean): Int = foldLeft(0)((n, a) => if (f(a)) n + 1 else n)
  }
}

@main def paramforwarding(): Unit = {
  println(new Derived(1, 2).sum)
  println(new Unused(3).one + new Unused(3).x)
  println(new Chain(4).twice)
  println(new Renamed(5).get)
  println(new OverHidden(6).get)
  println(new Concrete("seven").len)
  println(new Nested(8).inner.outerX)
  println(new Lifted.Ops(List(1, 2, 3, 4)).count(_ % 2 == 0))
}
