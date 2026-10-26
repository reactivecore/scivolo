//> using scala 3.9.0
import java.util.function.{Consumer, IntConsumer}

trait Named:
  def toString(): String
  def equals(o: Any): Boolean

class N(n: String) extends Named:
  override def toString(): String = n
  override def equals(o: Any): Boolean = o match
    case that: N => that.toString() == n
    case _       => false

abstract class Labelled(label: String):
  def name(): String = label

final class Item(name: String, val weight: Int) extends Labelled(name):
  override def toString() = this.name

trait It[T]:
  def next(): T

trait OfInt extends It[java.lang.Integer]:
  def nextInt(): Int
  def next() = nextInt()
  def each(action: Consumer[? >: java.lang.Integer]): Unit = action.accept(next())

class Counter(start: Int, step: Int):
  import Counter.*
  def this(start: Int) = this(start, Counter.defaultStep)
  def twice: Int = defaultStep * 2
  def value: Int = start + step

object Counter:
  val defaultStep = 2

def boxedConsumer(action: Consumer[? >: Integer]): IntConsumer = (action.accept(_)): IntConsumer

@main def Nj26members(): Unit =
  val n: Named = N("a")
  println(n.toString())
  println(n.equals(N("a")))
  val items = List(Item("x", 1), Item("y", 2))
  println(items.find(_.name() == "y").map(_.weight))
  println(items.head)
  val it = new OfInt { def nextInt() = 7 }
  it.each(i => println(i))
  println(new Counter(3).value)
  boxedConsumer(i => println(i)).accept(4)
