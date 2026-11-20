abstract class A:
  type M[X]
  type F[X] = M[X]
  type H = F
  def mk(i: Int): H[Int]

object B extends A:
  type M[X] = List[X]
  def mk(i: Int): H[Int] = List(i, i)

class C:
  type T
  type P = [X] =>> (X, T)

def test =
  val l: B.F[Int] = List(1)
  val h: B.H[Int] = B.mk(2)
  val c: C { type T = Int } = new C { type T = Int }
  type Q = c.P
  val q: Q[String] = ("q", 3)
  (l.head + h.head, q._2)
