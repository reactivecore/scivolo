//> using scala 3.9.0
class Outer(val base: Int):
  class Inner(val v: Int):
    def sum: Int = base + v
  object N:
    val mk = (k: Int) => new Inner(k)
  def local(x: Int): Int =
    class Loc:
      val f = (k: Int) => new Inner(k + x)
    Loc().f(1).sum

object Static:
  class Inner(val v: Int)
  class C:
    def mk = (k: Int) => new Inner(k)

@main def nestedobjlambda(): Unit =
  val o = Outer(10)
  println(o.N.mk(5).sum)
  println(o.local(2))
  println(Static.C().mk(4).v)
