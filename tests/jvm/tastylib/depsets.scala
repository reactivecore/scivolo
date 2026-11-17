import language.experimental.captureChecking

class It[A]:
  def map[B](f: A => B): It[B]^{this, f} = new It[B]
  def next(): A = ???

object DepSets:
  def from[E](it: It[E]^): It[E]^{it} = it match
    case it: (It[E @unchecked]^{it}) => it
    case _ => new It[E]
  def loop(f: It[Int] => It[Int]): It[It[Int]]^{f} =
    val it = new It[It[Int]]
    it.map(f)
  def dep(c: It[Int]^) = c.map(x => x)
