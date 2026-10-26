//> using scala 3.9.0
import scala.runtime.{BoxedUnit, IntRef, LongRef, Statics, VolatileBooleanRef}

class Meters(d: Int) extends AnyVal:
  inline override def toString(): String = s"$d m"

final class Count(val n: Int) extends AnyVal:
  def isZero = n == 0

class Units(scale: Int):
  opaque type Len = Int
  def len(i: Int): Len = i * scale
  def show(l: Len): String = s"${l}u"
  extension (l: Len) def +(m: Len): Len = l + m

def load(x: IntRef): Int = x.elem
def store(x: IntRef): Unit = x.elem = 42
def lazily(c: => Count): Count = c

@main def Nb24namerstubs(): Unit =
  println(Statics.finalizeHash(Statics.mix(17, 42), 1))
  println(Statics.anyHash("a"))
  println(Statics.longHash(1L << 40))
  val r = IntRef.create(3)
  r.elem += 4
  println(r.elem)
  store(r)
  println(load(r))
  val l = new LongRef(5L)
  println(l.elem + LongRef.zero().elem)
  println(VolatileBooleanRef.create(true).elem)
  println(classOf[BoxedUnit].getName)
  Statics.releaseFence()
  val m: Any = Meters(2)
  println(m)
  println(lazily(Count(0)).isZero)
  val u = Units(3)
  val a = u.len(2)
  import u.+
  println(u.show(a + u.len(1)))
