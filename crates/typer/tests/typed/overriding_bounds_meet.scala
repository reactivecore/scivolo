trait A { type T <: Comparable[T]; def x: T }
trait B extends A { type T <: Comparable[T] & CharSequence; def y: T = x }
trait C extends A { type T <: Comparable[T] { def isEmpty(): Boolean } }
abstract class E extends B with C
abstract class F extends C with B

object Use {
  def f(c: CharSequence) = "cs"
  def f(a: Any) = "any"
  def e1(e: E) = f(e.x) + e.x.length + e.x.isEmpty()
  def e2(e: E) = f(e.y)
  def f1(e: F) = f(e.y) + e.y.isEmpty()
}
