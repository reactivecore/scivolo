trait TC[T]
object TC:
  given TC[Int]()

class Box

given box: Box = Box()

trait Plus[X, R]:
  def add(x: X): R

object O:
  class Inst[T]:
    def unapply(x: Any): Option[T] = None

object Test:
  def f[T: TC]: TC[T] = summon
  def g(using x: TC[Int]): TC[Int] = implicitly
  val h: TC[Int] = summon
  def b = summon[Box]
  def p[Y]: Plus[Int, Y] = new { def add(x: Int): Y = ??? }
  def i = O.Inst[Int]()
