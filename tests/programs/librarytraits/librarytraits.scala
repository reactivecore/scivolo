//> using scala 3.9.0
class Half extends (Double => Double):
  def apply(x: Double) = x / 2
class Answer extends (() => Int):
  def apply() = 42
class Add extends ((Int, Int) => Int):
  def apply(a: Int, b: Int) = a + b
class Show extends (Int => String):
  def apply(x: Int) = "<" + x + ">"
class Susp[+A](value: => A) extends Function0[A]:
  def apply() = value
trait Inc extends (Int => Int):
  def apply(x: Int) = x + 1
class UseInc extends Inc
class Positive extends PartialFunction[Int, String]:
  def isDefinedAt(x: Int) = x > 0
  def apply(x: Int) = "positive " + x
type I = Int
class Aliased extends (I => I):
  def apply(x: I) = x * 3
class Captured extends (Int => Int):
  def apply(x: Int) =
    class Local:
      def v = x + 1
    new Local().v
object Twice extends (Int => Int):
  def apply(x: Int) = 2 * x
object Numbers extends Iterable[Int]:
  def iterator = Iterator(1, 2, 3)
class Quiet(msg: String) extends Exception(msg):
  override def fillInStackTrace(): Throwable = this

class Version(val n: Int) extends Ordered[Version]:
  def compare(that: Version) = n - that.n
class ByLength extends Ordering[String]:
  def compare(a: String, b: String) = a.length - b.length
class Count(limit: Int) extends Iterator[Int]:
  var i = 0
  def hasNext = i < limit
  def next() = { i += 1; i }
class Letters extends Iterable[String]:
  def iterator = Iterator("a", "b")
class Shorter extends java.util.Comparator[String]:
  def compare(a: String, b: String) = a.length - b.length

object Fruit extends Enumeration:
  val Apple, Pear = Value

class Cell(var value: Int, val step: Int):
  value += step
  def bump(): Unit = value = value + step
case class Box(var content: String)

trait Cached:
  lazy val label = { println("computing label"); "cached" }
  lazy val size: Int = label.length
  private lazy val hidden = label + "!"
  def reveal = hidden
trait Named extends Cached:
  lazy val name = "named " + label
  val plain = 3
class Plain extends Cached
class Both extends Object with Named:
  def all = label + " " + name + " " + plain
class Overriding extends Cached:
  override lazy val label = "own"

class Temporary:
  private val (first, second) = (2, 3)
  val sum = first + second
  private val scale = 10
  val scaled = (x: Int) => x * scale

@main def librarytraits(): Unit =
  println(new Half()(5.0))
  println(new Answer()())
  println(new Add()(1, 2))
  println(new Show()(4))
  println(new Susp("lazy")())
  println(new UseInc()(1))
  val f: Int => Int = new UseInc
  println(f(7))
  println(new Positive()(5))
  println(new Aliased()(2))
  println(new Captured()(2))
  println(Twice(4))
  println(Numbers.sum)
  println(new Quiet("quiet").getMessage)
  println(new Version(1) < new Version(2))
  println(new ByLength().max("abc", "de"))
  println(new Count(3).toList)
  println(new Letters().mkString(","))
  println(new Shorter().compare("ab", "c"))
  println(Fruit.values)
  println(Fruit.withName("Pear").id)
  val c = new Cell(1, 2)
  c.bump()
  println(c.value)
  val b = Box("x")
  b.content = "y"
  println(b)
  val p = new Plain
  println(p.size)
  println(p.reveal)
  println(new Both().all)
  println(new Overriding().label)
  val t = new Temporary
  println(t.sum + t.scaled(2))
