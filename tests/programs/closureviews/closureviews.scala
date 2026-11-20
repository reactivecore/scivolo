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

object closureviews:
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
