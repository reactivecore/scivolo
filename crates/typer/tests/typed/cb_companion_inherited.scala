//> using options -language:experimental.modularity -source future
trait M:
  type Self
  def unit: Self

trait Num:
  type Self
  def zero: Self

trait AAA:
  type X: M
trait BBB:
  type X: Num

class CCC[X1: {M, Num}] extends AAA, BBB:
  type X = X1
  val z = X.zero
  val u = X.unit
