trait A:
  class B

abstract class P:
  val a: A
  val b: a.B

object P:
  def unapply(that: P): Option[(that.a.type, that.a.B)] = Some((that.a, that.b))

object anA extends A

object Test:
  object t extends P:
    val a = anA
    val b = new a.B

  def f = t match
    case P(u, v) => u: A
