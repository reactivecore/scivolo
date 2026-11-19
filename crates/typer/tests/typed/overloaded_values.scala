trait Ap {
  def apply(x: AnyRef): Int
}

object ValueApply {
  def f(e: Any): Ap = ???
  val f: Ap = ???
  val g: Ap = f(null)
}

trait Plain
trait Takes[T] {
  def apply(f: T => T): Unit
  def apply(s: String): Unit
}

object ResultApply {
  def v: Plain = ???
  def v[T]: Takes[Int] = ???
  def use = v(x => x + 1)
}

object Spread {
  def foo(a1: String*) = a1
  val h = (foo: Seq[String] => Seq[String])
}
