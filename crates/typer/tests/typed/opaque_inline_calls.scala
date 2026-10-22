type Amount = Amount.Type
object Amount:
  opaque type Type = Int
  inline def twice(x: Type): Type = x + x
  def of(n: Int): Type = n

object foos:
  opaque type Foo[T] = String
  object bars:
    class Bar1[A]
    inline def mkBar1[A]: Bar1[A] = new Bar1[A]

object outer:
  opaque type Queue = Queue.Unsafe
  object Queue:
    abstract class Unsafe
    opaque type Unbounded = Queue
    object Unbounded:
      inline def initWith()(f: Unbounded => Unit): Unit = ()

def test(c: Amount): Unit =
  Amount.twice(c)
  Amount.twice(Amount.of(1))
  val b = foos.bars.mkBar1[Int]
  outer.Queue.Unbounded.initWith() { q => () }
