//> using options -source future -language:experimental.modularity
trait Ordering:
  type T

class SetFunctor(tracked val ord: Ordering):
  type Set = List[ord.T]
  def empty: Set = Nil

object intOrdering extends Ordering:
  type T = Int

val intSet = new SetFunctor(intOrdering)

def ints: List[Int] = intSet.empty
