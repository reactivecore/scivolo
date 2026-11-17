import language.experimental.captureChecking
class It[+T]:
  def next(): T = ???
trait Coll[+T]:
  def iterator: It[T]^{this}
trait Src:
  def it: It[Int]^{this}
  def two(a: Coll[Int]^, b: Coll[Int]^): It[Int]^{a, b}
class Q(c: Coll[Int]^) extends Src:
  def it = ???
  def two(a: Coll[Int]^, b: Coll[Int]^) = a.iterator
class R extends Coll[Int]:
  def iterator = ???
