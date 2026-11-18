class CoBox[+A]:
  def get: A = ???
  def self: CoBox[A] = this
class InvBox[A]:
  def get: A = ???
  def self: InvBox[A] = this
  def list: List[A] = ???
class ContraBox[-A]:
  def self: ContraBox[A] = this
class SubCo[+B] extends CoBox[B]
class SubSub[+B] extends SubCo[B]
class SubInv[B] extends InvBox[B]
class SubContra[-B] extends ContraBox[B]
class NestedInv[B] extends CoBox[InvBox[B]]
class InvOfInv[B] extends InvBox[InvBox[B]]
class InvOfCo[B] extends InvBox[CoBox[B]]
class CoOfContra[B] extends CoBox[ContraBox[B]]
class Fun[B] extends CoBox[B => B]
class Pair2[A, +B] extends CoBox[(A, B)]

class WildcardMembers:
  def iterator(that: Iterable[?]) = that.iterator
  def local(that: Iterable[?]): Int =
    val thatIt = that.iterator
    var n = 0
    while thatIt.hasNext do
      thatIt.next()
      n += 1
    n
  def bounded(that: Iterable[? <: String]) = that.iterator
  def head(that: Iterable[?]) = that.head
  def entries(that: Map[?, ?]) = that.iterator
  def values(that: Map[String, ?]) = that.iterator
  def covariant(b: SubCo[?]) = b.get
  def covariantSelf(b: SubCo[?]) = b.self
  def twice(b: SubSub[? <: String]) = b.get
  def invariant(b: SubInv[?]) = b.get
  def invariantSelf(b: SubInv[?]) = b.self
  def invariantList(b: SubInv[?]) = b.list
  def contravariant(b: SubContra[?]) = b.self
  def lowerBound(b: SubContra[? >: String]) = b.self
  def nested(b: NestedInv[?]) = b.get
  def invOfInv(b: InvOfInv[?]) = b.get
  def invOfCo(b: InvOfCo[? <: String]) = b.list
  def coOfContra(b: CoOfContra[?]) = b.get
  def function(b: Fun[?]) = b.get
  def tuple(b: Pair2[Int, ?]) = b.get
