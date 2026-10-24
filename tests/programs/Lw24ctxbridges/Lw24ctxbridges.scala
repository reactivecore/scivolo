//> using scala 3.9.0
class Lw24A
class Lw24B

trait Lw24Foo:
  def foo: Lw24A ?=> Lw24B ?=> Int

class Lw24Foo1 extends Lw24Foo:
  def foo: Lw24A ?=> Lw24B ?=> Int = 1

class Lw24Bar[T]:
  def bar: Lw24A ?=> T = null.asInstanceOf[T]

class Lw24Bar1 extends Lw24Bar[Lw24B ?=> Int]:
  override def bar: Lw24A ?=> Lw24B ?=> Int = 2

object Lw24ctxbridges:
  def main(args: Array[String]): Unit =
    given Lw24A = Lw24A()
    given Lw24B = Lw24B()
    val f: Lw24Foo = Lw24Foo1()
    println(f.foo)
    val b: Lw24Bar[Lw24B ?=> Int] = Lw24Bar1()
    println(b.bar)
    println(Lw24Bar1().bar)
