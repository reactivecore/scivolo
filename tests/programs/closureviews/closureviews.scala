//> using scala 3.9.0
import scala.language.implicitConversions

trait Fn[-T, R] { def apply(t: T): R }

final class Coll[+A](val items: List[A]):
  def map[B](f: A => B): Coll[B] = new Coll(items.map(f))
  def lazyZip[B](that: Coll[B]): Zipped2[A, B] = new Zipped2(this, that)

final class Zipped2[+X, +Y](val xs: Coll[X], val ys: Coll[Y]):
  def map[B, C](f: (X, Y) => B): Coll[C] = new Coll[C](Nil)

object Zipped2:
  implicit def toColl[X, Y](z: Zipped2[X, Y]): Coll[(X, Y)] = new Coll(z.xs.items.zip(z.ys.items))

class Duration(val n: Int)
class Clock(val t: Int):
  def plus(a: Int): Clock = new Clock(t + a)

implicit class ClockOps(c: Clock):
  def plus(d: Duration): Clock = new Clock(c.t + 100 * d.n)

trait Printable:
  def pprint(v: () => String): Unit = println("member " + v())

extension (p: Printable)
  def pprint(f: () => Int): Unit = p.pprint(() => "extension " + f())

opaque type Position[Buffer] = Int
object Position:
  def of[B](c: Char): Position[B] = c.toInt + 1000
  def show[B](p: Position[B]): String = "position " + p

object Positions:
  given FromChar: Conversion[Char, Position[CharSequence]] = c => Position.of(c)

abstract class Dim
object NoDim extends Dim
object Hour extends Dim:
  override def toString = "Hour"
case class Measure(scalar: Double, unit: Dim):
  def *(newUnit: Dim): Measure = Measure(scalar * 2, newUnit)
implicit def double2Measure(scalar: Double): Measure = Measure(scalar, NoDim)

class Op[I]:
  override def toString = "op"
class IntOp extends Op[Int]
class Rich(x: Double):
  def +(op: IntOp): String = s"$x + int $op"
  def +[I](op: Op[I]): String = s"$x + $op"
implicit def toRich(x: Double): Rich = new Rich(x)

class Target
implicit class Wrong(t: Target):
  def right: Any = "wrong"
implicit class Right(t: Target):
  def right(node: Any): Any = "right " + node

trait Ctx

class Single:
  def m(a: Nothing): Int = 0

implicit class RichAny(a: Any):
  def m(a: Any): Int = 1

object closureviews:
  def foo(f: Ctx => Int) = "plain " + f(new Ctx {})
  @annotation.targetName("fooContextual")
  def foo(f: Ctx ?=> Int) = "contextual " + f(using new Ctx {})

  def sum(x: Int, y: => Int) = x + y + y
  def total(i: Int*) = i.sum
  type T2 = Tuple2[Int, Int]

  def main(args: Array[String]): Unit =
    var evaluated = 0
    val byName = sum(2, _)
    println(byName({ evaluated += 1; 10 }))
    println(evaluated)
    val repeated = i => total(i*)
    println(repeated(Seq(1, 2, 3)))
    val tupled: T2 => Int = (x1, x2) => x1 * 10 + x2
    println(tupled((3, 4)))
    val partial: PartialFunction[?, Any] = { case i => i }
    println(partial.asInstanceOf[PartialFunction[Any, Any]]("pf"))
    val refined: Fn[Int, Int] { def apply(arg: Int): Int } = x => x + 1
    println(refined(41))

    val xs = new Coll(List(1, 2, 3))
    println(xs.lazyZip(xs).map((x: (Int, Int)) => x._1 * x._2).items)
    val b = args.isEmpty
    println(new Clock(1).plus(if b then Duration(2) else Duration(3)).t)
    val byte: Byte = 97
    println("abc" contains byte)
    println("abc" contains 'a')
    val p = new Printable {}
    p.pprint(() => "s")
    p.pprint(() => { 123 })
    println(foo(ctx => 1))
    println(foo(ctx ?=> 2))
    println((new Single).m({ case (x, y) => x }: Any => Any))
    import Positions.given
    val position: Position[CharSequence] = 'x'
    println(Position.show(position))
    println(3.0 * (Hour: Dim))
    println(1.0 + new Op[Int])
    println(new Target().right(List(1)))
    val tupledContains = "".contains("", (_: Int))
    println(tupledContains(0))
