//> using options -language:experimental.modularity -source future
trait M:
  type Self
  def unit: Self

trait A extends M
trait B extends M

trait AA:
  type XX: {M, A, B}
  val x = XX.unit

class C

given C is M:
  def unit = C()

given C is A:
  def unit = C()

given C is B:
  def unit = C()

class CC extends AA:
  type XX = C
