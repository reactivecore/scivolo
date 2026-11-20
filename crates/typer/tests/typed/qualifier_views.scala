import scala.language.implicitConversions

final class Coll[+A]:
  def map[B](f: A => B): Coll[B] = new Coll[B]
  def lazyZip[B](that: Coll[B]): Zipped2[A, B] = new Zipped2[A, B](this, that)

final class Zipped2[+X, +Y](xs: Coll[X], ys: Coll[Y]):
  def map[B, C](f: (X, Y) => B): Coll[C] = new Coll[C]

object Zipped2:
  implicit def toColl[X, Y](zipped2: Zipped2[X, Y]): Coll[(X, Y)] = new Coll[(X, Y)]

class Amount(val n: Int)
class Clock:
  def plus(a: Amount): Clock = this

class Duration(val n: Int)
implicit class ClockOps(c: Clock):
  def plus(d: Duration): Clock = c

trait Printable:
  def pprint(v: () => String): Unit = println(v())

extension (p: Printable)
  def pprint(f: () => Int): Unit = p.pprint(() => f().toString)

object QualifierViews:
  def zipped(xs: Coll[Int]) = xs.lazyZip(xs).map((x: (Int, Int)) => x._1 + x._2)
  def conditional(c: Clock, b: Boolean) = c.plus(if b then Duration(1) else Duration(2))
  def contains(b: Byte) = "abc" contains b
  def printed(p: Printable) = p.pprint(() => { 123 })
