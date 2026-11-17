import language.experimental.captureChecking

class It[A]:
  def map[B](f: A => B): It[B]^{this, f} = new It[B]
  def next(): A = ???

abstract class AbsIt[+A]:
  def next(): A

class Slide[A](l: It[A]^, size: Int) extends AbsIt[It[A]^{l}]:
  private var cur: It[A]^{l} = l
  def next(): It[A]^{l} = cur

object DepSets:
  def from[E](it: It[E]^): It[E]^{it} = it match
    case it: (It[E @unchecked]^{it}) => it
    case _ => new It[E]
  def loop(f: It[Int] => It[Int]): It[It[Int]]^{f} =
    val it = new It[It[Int]]
    it.map(f)
  def dep(c: It[Int]^) = c.map(x => x)