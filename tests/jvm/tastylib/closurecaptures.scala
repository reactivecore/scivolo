import language.experimental.captureChecking

class A
class Label[T]
trait Coll:
  def label: Label[Int]^{this}

object Closures:
  def g() =
    val f = (x: A^) => 1
    f
  def h() =
    val f = (x: Coll^) => x.label
    1
  val poly = [T] => (x: A^) => 1
  val top = (x: A^) => 1
  def curried = (x: A^) => (y: Coll^) => 1
  val dep: (x: Coll^) -> Label[Int]^{x} = x => x.label
  def plain = (x: A) => x
  val two: (x: Coll^, y: Coll^) -> Label[Int]^{x, y} = (x, y) => x.label
  val equalFormals = (x: A^, y: A^, z: A^) => 2
  def block() =
    val f = (x: Coll^) => x.label
    f
  def nestedBlock() =
    val r = { val f = (x: Coll^) => x.label; f }
    r
  def lastClosure = { (x: Coll^) => x.label }
  def dependentCurried = (x: Coll^) => (y: Coll^) => x.label
  val threeCurried = (x: A^) => (y: A^) => (z: A^) => 1
  def capturing(c: Coll^) = (x: A^) => (y: A^) => c.label
