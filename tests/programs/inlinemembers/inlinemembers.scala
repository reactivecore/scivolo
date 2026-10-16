abstract class Shape:
  def area(scale: Int): Int = 0
  def name: String
  inline def tag(x: Int): Int

class Square(side: Int) extends Shape:
  inline override def area(scale: Int): Int =
    val s = side * scale
    s * s
  inline def name: String = "square"
  inline def tag(x: Int): Int = x + side

trait Base:
  def weight: Int = 1

trait Heavy extends Base:
  inline override def weight: Int = 10

class Crate extends Heavy:
  def total: Int = super.weight + weight

class Counter(private val start: Int):
  private var count = start
  protected def step: Int = 2
  inline def next(): Int =
    count = count + step
    count
  inline def peek: Int = count + start

object Registry:
  inline def lookup: String = Hidden.secret
  private object Hidden:
    def secret = "found"

class Outer(val k: Int):
  class Inner:
    inline def twice: Int = k * 2

trait Show[T]:
  def show(t: T): String

class IntShow extends Show[Int]:
  def show(t: Int): String = "int " + t

object Show:
  inline given Show[Int] = IntShow()

object inlinemembers:
  def describe(s: Shape): String = s"${s.name} ${s.area(2)}"

  def main(args: Array[String]): Unit =
    val sq = Square(3)
    println(describe(sq))
    println(sq.tag(4))
    println(sq.area(1))
    val c = Crate()
    println(c.total)
    println((c: Base).weight)
    val counter = Counter(5)
    println(counter.next())
    println(counter.next())
    println(counter.peek)
    println(Registry.lookup)
    val outer = Outer(4)
    val inner = outer.Inner()
    println(inner.twice)
    val shown = summon[Show[Int]]
    println(shown.show(7))
