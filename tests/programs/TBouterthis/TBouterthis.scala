trait GetInt:
  def value: Int

class A(val x: Int):
  class X:
    inline def foo() = A.this.get() + 1
  private def get(): Int = x
  val g = new GetInt:
    inline def value: Int = x * 2

class B extends A(20):
  val a = new A(10)
  class Y extends a.X
  def y(): Y = new Y

trait S { self =>
  def name: String
  class Foo:
    inline def show: String = self.name
}

class S2 extends S:
  def name = "S2"
  val s = new S2a
  def shown = (new s.Foo).show

class S2a extends S:
  def name = "S2a"

class Outer:
  val outer = 3
  object Inner:
    inline def inlined: Int = outer
    def forwarder: Int = inlined

@main def TBouterthis(): Unit =
  var b = new B
  println(b.y().foo())
  println(b.g.value)
  println(b.a.g.value)
  println(new S2().shown)
  println((new Outer).Inner.forwarder)
