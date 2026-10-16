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

class Ord(val s: String) extends Comparable[Ord]:
  def compareTo(other: Ord) = s.compareTo(other.s)

object Test:
  def sorted = List(Ord("b"), Ord("a")).sorted
  def f[T: TC]: TC[T] = summon
  def g(using x: TC[Int]): TC[Int] = implicitly
  val h: TC[Int] = summon
  def b = summon[Box]
  def p[Y]: Plus[Int, Y] = new { def add(x: Int): Y = ??? }
  def i = O.Inst[Int]()
  val m = summon[scala.deriving.Mirror.Of[(Int, String)]]
  def seconds =
    import scala.concurrent.duration.*
    List((60.seconds, 1.minute))
  def n: (Int, String) = m.fromProduct((1, "a"))
