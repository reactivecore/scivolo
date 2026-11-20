class A
object A:
  given a1: A = A()
  given a2: A = A()

class B
object B:
  given b: B = B()

class R
trait LowR:
  given viaA(using a: A): R = R()
object R extends LowR:
  given viaB(using b: B, b2: B): R = R()

class Q
object Q:
  given q(using r: R): Q = Q()

object Test:
  val r = summon[R]
  val q = summon[Q]
