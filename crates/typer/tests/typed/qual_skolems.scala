trait Foo {
  type State
  def bar(f: Bar[State] => Bar[State]): Foo = this
}
class Bar[+A]

object Main {
  class C {
    type T
    def foo(x: T): T = x
    def get: T = ???
  }
  var c = new C
  def unit: Foo = new Foo { type State = Any }
  def main(args: Array[String]): Unit = {
    def y = c.foo(???)
    def z = c.get
    val f = unit.bar(b => b)
    println(f)
  }
}
