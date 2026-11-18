object O:
  opaque type T = Int
  val offset = 1
  def mk(i: Int): T = i
  inline def plain(x: T): Int = x
  inline def helper(x: T): Int = x + offset
  inline def viaThis(x: T): Int = helper(x) + offset
  inline def viaPlain(x: T): Int = plain(x) + 1
  extension (x: T)
    inline def raw: Int = x
    inline def twice: Int = x.raw + x.raw

class C:
  opaque type U = String
  def mk(s: String): U = s
  inline def len(u: U): Int = u.length
  inline def lenTwice(u: U): Int = len(u) + len(u)

object Outer:
  inline def useO(i: Int): Int = O.viaThis(O.mk(i)) + O.mk(i).twice

object Test:
  val a = O.viaThis(O.mk(1))
  val b = O.viaPlain(O.mk(2))
  val c = O.mk(3).twice
  val d = Outer.useO(4)
  val e = new C
  val f = e.lenTwice(e.mk("abc"))
