package pk

class Foo
trait T { type A = Foo; type L = List[Foo] }

object O {
  type B = T#A
  type C = T#L
  def b(x: B): Foo = x
  def c(x: C): List[Foo] = x
}
