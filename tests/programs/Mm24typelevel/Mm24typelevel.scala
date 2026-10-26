//> using scala 3.9.0
// Mirror element types in inline matches, match types over paths, compile-time operations over
// type parameters, recursive match type aliases, `codeOf`, and generic sealed companions.

import scala.compiletime.*
import scala.compiletime.ops.int.S
import scala.deriving.Mirror

trait Mm24Size[T]:
  def size: Int

object Mm24Size:
  inline def count[T <: Tuple]: Int = inline erasedValue[T] match
    case _: EmptyTuple => 0
    case _: (t *: ts) => 1 + count[ts]

  inline given derived: [T] => (m: Mirror.Of[T]) => Mm24Size[T] =
    val n = count[m.MirroredElemTypes]
    new Mm24Size[T]:
      def size = n

enum Mm24Opt[+T]:
  case Sm(t: T)
  case Nn

object Mm24Opt:
  given derivedSize: [T] => Mm24Size[Mm24Opt[T]] = Mm24Size.derived

abstract class Mm24Shape:
  type Elems <: Tuple
class Mm24Pair extends Mm24Shape:
  override type Elems = (Int, String)

inline def mm24Elems(s: Mm24Shape): Int = Mm24Size.count[s.Elems]

enum Mm24Nat:
  case Zero
  case Succ[N <: Mm24Nat.Refract](n: N)

object Mm24Nat:
  type Refract = Zero.type | Succ[?]

inline def mm24ToInt[N <: Mm24Nat.Refract](inline nat: N): Int = inline nat match
  case Mm24Nat.Zero => 0
  case Mm24Nat.Succ(n) => mm24ToInt(n) + 1

def mm24PlusOne[I <: Int](using x: ValueOf[S[I]]): S[I] = x.value

type Mm24Leaf[X] = X match
  case String => Char
  case List[t] => Mm24Leaf[t]

sealed trait Mm24Sum[A]
object Mm24Sum:
  case class One[A](a: A) extends Mm24Sum[A]
  case class Zero[A]() extends Mm24Sum[A]

sealed trait Mm24Item
case class Mm24A(i: Int) extends Mm24Item
case object Mm24B extends Mm24Item

object Mm24typelevel {
  def main(args: Array[String]): Unit = {
    println(summon[Mm24Size[Mm24Opt[Int]]].size)
    val pair = new Mm24Pair
    println(mm24Elems(pair))
    println(mm24ToInt(Mm24Nat.Succ(Mm24Nat.Succ(Mm24Nat.Zero))))
    println(mm24PlusOne[2])
    val leaf: Mm24Leaf[List[List[String]]] = 'x'
    println(leaf)
    println(codeOf(1 + 2))
    println(codeOf("a\tb"))
    println(codeOf(println(34)))
    val m = summon[Mirror.SumOf[Mm24Sum[Int]]]
    println(m.ordinal(Mm24Sum.One(1)))
    println(m.ordinal(Mm24Sum.Zero()))
    val items = summon[Mirror.Of[Mm24Item]]
    val second = summon[Mirror.Of[Tuple.Elem[items.MirroredElemTypes, 1]]]
    println(constValue[second.MirroredLabel])
    val first: Tuple.Head[items.MirroredElemTypes] = Mm24A(3)
    println(first)
  }
}
