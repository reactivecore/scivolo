trait Foo:
  type T

case class S(a: Foo, b: a.type, c: List[a.T])

class O(val o: Foo):
  case class I(a: Foo, b: a.T, c: o.T)
