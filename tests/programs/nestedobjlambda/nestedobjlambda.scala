//> using scala 3.9.0
class Outer(val base: Int):
  class Inner(val v: Int):
    def sum: Int = base + v
  object N:
    val mk = (k: Int) => new Inner(k)

@main def nestedobjlambda(): Unit =
  val o = Outer(10)
  println(o.N.mk(5).sum)
