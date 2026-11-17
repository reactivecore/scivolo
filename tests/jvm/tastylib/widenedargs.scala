import language.experimental.captureChecking

class B
class It[+T]:
  def next(): T = ???
trait Coll[+T]:
  def iterator: It[T]^{this}

object W:
  def id[T](x: T): T = x
  def mk(c: Coll[Int]^): It[Int]^{c} = c.iterator
  def g(b: B^, c: Coll[Int]^, k: Boolean) =
    val a1 = id(mk(c))
    val a2 = id(if k then b else b)
    val a4 = Some(mk(c))
    val a5 = List(mk(c), mk(c))
    val a6 = id(c.iterator: It[Int]^{c})
    val a7 = id(id(b))
    val a8 = mk(c)
    val a9 = id(c.iterator)
    val a11 = id(c)
    1
