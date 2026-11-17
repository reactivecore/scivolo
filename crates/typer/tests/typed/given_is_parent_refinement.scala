import language.experimental.modularity

trait M:
  type Self
  extension (x: Self) def combine(y: Self): String
  def unit: Self

class C

given C is M:
  extension (x: Self) def combine(y: Self) = "M"
  def unit = C()

def test =
  class D
  given D is M:
    extension (x: Self) def combine(y: Self) = "D"
    def unit = D()
  summon[D is M].unit
