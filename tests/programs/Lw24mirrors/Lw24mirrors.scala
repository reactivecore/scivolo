//> using scala 3.9.0
import scala.deriving.Mirror
import scala.compiletime.{erasedValue, summonInline}

inline def lw24Count[T <: Tuple]: Int = inline erasedValue[T] match
  case _: EmptyTuple => 0
  case _: (t *: ts) => 1 + lw24Count[ts]

trait Lw24Eq[-T]:
  def weight: Int

object Lw24Eq:
  inline given derived: [T] => (m: Mirror.Of[T]) => Lw24Eq[T] =
    val n = lw24Count[m.MirroredElemTypes]
    new Lw24Eq[T]:
      def weight = n + 1

trait Lw24Size[-T]:
  def size: Int

object Lw24Size:
  inline given derived: [T] => (m: Mirror.Of[T]) => Lw24Size[T] =
    val n = lw24Count[m.MirroredElemTypes]
    new Lw24Size[T]:
      def size = n

enum Lw24Opt[+T]:
  case Sm(t: T)
  case Nn

object Lw24Opt:
  given derivedEq: [T] => Lw24Eq[Lw24Opt[T]] = Lw24Eq.derived
  def sizes: Int = summonInline[Lw24Size[Lw24Opt.Nn.type]].size

object Lw24mirrors:
  def main(args: Array[String]): Unit =
    println(summon[Lw24Eq[Lw24Opt[Int]]].weight)
    println(Lw24Opt.sizes)
