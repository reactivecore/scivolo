//> using options -source future
object Defs:
  import language.experimental.modularity

  trait M:
    type Self
    extension (x: Self) def combine(y: Self): String
    def unit: Self

  trait A extends M
  trait B extends M

  def f[X: {M, A, B}](x: X) =
    summon[X is M]
    x.combine(X.unit)

  def g[X: M as m](x: X): X = X.unit
