class Base
class Sub extends Base

def sub(): Sub = new Sub
def base(): Base = new Base

object M:
  inline def wider(a: Sub, b: Base): List[b.type] = { println(a); Nil }
  inline def same(a: Base, b: Base): List[b.type] = { println(a); Nil }

object Test:
  val x = M.wider(sub(), base())
  val y = M.same(base(), base())
