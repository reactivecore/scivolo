class A { def f(x: Int): Int = x + 1; override def toString = "A" }
trait T extends A { override def f(x: Int): Int = super.f(x) * 2 }
trait U extends A { override def f(x: Int): Int = super.f(x) - 1 }
class B extends A with T with U {
  override def f(x: Int): Int = super.f(x) + 100
  def g = super[T].f(1) + super[A].f(1)
  override def toString = super.toString + "B"
  class Inner { def c() = B.super[T].f(2) + B.super.f(3) }
}
trait Box[V] { def put(a: V): String = s"box($a)" }
trait Logged[V] extends Box[V] { override def put(a: V): String = "log:" + super.put(a) }
