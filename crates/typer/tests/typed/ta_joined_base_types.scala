trait Like[+A, +C[_]]:
  def coll: C[A] @annotation.unchecked.uncheckedVariance
trait It[+A] extends Like[A, It]
trait Sq[+A] extends It[A] with Like[A, Sq]

trait Sink[-A]
trait Ints extends Sink[Int]
trait Anys extends Ints with Sink[Any]

object Test:
  def coll(s: Sq[Int]): Sq[Int] = s.coll
  def sink(s: Anys): Sink[Any] = s
