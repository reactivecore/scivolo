import language.experimental.captureChecking
class A
class B
class Box[T](val x: T)
class S:
  def me: this.type = this
class It[+T]:
  def next(): T = ???
trait Coll[+T]:
  def iterator: It[T]^{this}
trait Fac[+C]:
  def make(it: Coll[Int]^): C^{it}
class Mk extends Fac[It[Int]]:
  def make(it: Coll[Int]^) = it.iterator
final class ByNames(m: => Int, b: => B):
  private val e = m
  private val f = b
  def g = m
object G:
  def first[T](xs: List[T]): T = xs.head
  def box[T](x: T): Box[T] = new Box[T](x)
  def pick(x: B^): x.type = x
  def mk(c: Coll[Int]^): It[Int]^{c} = c.iterator
  def mk2(c: Coll[Int]^): It[Int]^{c} = c.iterator
  def g(a: A^, b: B^{a}, s: S^{a}, c: Coll[Int]^, d: Coll[Int]^, k: Boolean): Int =
    val y = first(List(b))
    val z: Box[B^{a}] = box(b)
    val w = box(b)
    val m = s.me
    val p = pick(b)
    val a9 = if k then mk(c) else c.iterator
    val a10 = if k then mk(c) else mk2(d)
    val cl = (i: Int) => mk(c)
    val app = List(1).map(i => mk(c))
    1
