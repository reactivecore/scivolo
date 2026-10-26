//> using scala 3.9.0
import scala.compiletime.ops.int.*
import scala.deriving.Mirror

type Tc26Leaf[X] = X match
  case String => Char
  case Int => Int

def tc26Leaf[X](x: X): Tc26Leaf[X] = x match
  case s: String => s.charAt(0)
  case i: Int => i

type Tc26Domain
type Tc26Impl[T] <: Tc26Domain
type Tc26Extract[X] = X match { case Tc26Impl[t] => t }

trait Tc26Base[D <: Tc26Domain]:
  def f(pp: Tc26Extract[D]): Int

class Tc26Ext extends Tc26Base[Tc26Impl[String]]:
  def f(pp: String) = pp.length

def tc26Call[M <: Tc26Domain](rc: Tc26Base[M], v: Tc26Extract[M]): Int = rc.f(v)

sealed trait Tc26List
sealed trait Tc26Nil extends Tc26List
sealed trait :+:[+H, +T <: Tc26List] extends Tc26List

type Tc26Refl[T <: Tuple] = T match
  case EmptyTuple => Tc26Nil
  case h *: tl => h :+: Tc26Refl[tl]

class Tc26C3[T](val elem: T)
class Tc26D3[T](val elemD: T) extends Tc26C3[T](elemD)

def tc26Elem[T](x: Tc26C3[T]): T = x match
  case d: Tc26D3[t] => d.elemD
  case c => c.elem


trait Tc26Show[T]:
  def show(t: T): String

object Tc26Show:
  inline given derived: [T] => (m: Mirror.Of[T]) => Tc26Show[T] =
    new Tc26Show[T]:
      def show(t: T): String = scala.compiletime.constValue[m.MirroredLabel]

case object Tc26Thing:
  given Tc26Show[Tc26Thing.type] = Tc26Show.derived

abstract case class Tc26Abs(x: Int)

type Tc26One = 1

object Tc26matches:
  def Tc26Abs(x: Int): Tc26Abs = new Tc26Abs(x * 2) {}

  def main(args: Array[String]): Unit =
    val c: Char = tc26Leaf("leaf")
    val n: Int = tc26Leaf(7)
    println(s"$c$n")
    println(tc26Call(new Tc26Ext, "four"))
    val r: Tc26Refl[(Int, String)] = null.asInstanceOf[Int :+: String :+: Tc26Nil]
    println(r == null)
    println(tc26Elem(new Tc26D3("d")))
    println(tc26Elem(new Tc26C3("c")))
    println(summon[Tc26Show[Tc26Thing.type]].show(Tc26Thing))
    Tc26Abs(2) match
      case Tc26Abs(4) => println("abs")
    val two: Tc26One + Tc26One = 2
    println(two)
