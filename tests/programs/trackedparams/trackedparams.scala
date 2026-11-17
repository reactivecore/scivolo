//> using options -source future -language:experimental.modularity
trait Ordering:
  type T
  def compare(t1: T, t2: T): Int

class SetFunctor(tracked val ord: Ordering):
  type Set = List[ord.T]
  def empty: Set = Nil
  extension (s: Set)
    def add(x: ord.T): Set = x :: s.filter(e => ord.compare(x, e) != 0)

object intOrdering extends Ordering:
  type T = Int
  def compare(t1: T, t2: T): Int = t1 - t2

sealed abstract class Foo(tracked val discriminator: String)
class Bar extends Foo("bar")

@main def trackedparams(): Unit =
  val intSet = new SetFunctor(intOrdering)
  import intSet.*
  println(intSet.empty.add(6).add(8).add(6))
  val bar: Foo = Bar()
  println(bar.discriminator)
