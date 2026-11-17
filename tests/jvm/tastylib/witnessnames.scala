//> using options -source future
package scala.probe

@annotation.experimental
class WitnessNames:
  import language.experimental.modularity

  trait M:
    type Self
    def unit: Self

  def g[X: M as m](x: X): X = X.unit
  def h[X: {M as a, M as b}](x: X): X = a.unit
