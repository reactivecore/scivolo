trait Foo[A]

trait N {
  type A <: Foo[G.A]
  def a: A
}

object G extends N {
  def a: A = ???
}

case class Baz(n: N)

object U {
  def f(b: Baz) = b match {
    case Baz(n) => n.a
  }
}
