class Outer:
  class Inner[T](val t: T)

def test(b: Boolean, p: Outer, q: Outer) =
  val i = if b then p.Inner(1) else q.Inner(2)
  i.t
