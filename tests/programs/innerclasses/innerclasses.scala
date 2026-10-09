//> using scala 3.9.0
class Outer(val base: Int):
  def twice(x: Int): Int = x * 2
  class Inner(val k: Int):
    def sum: Int = base + twice(k)
  class Empty:
    def three: Int = 3
  private class Hidden(val n: Int):
    def n2: Int = n * 2
  class Cell[T](val v: T):
    def pair: (Int, T) = (base, v)
  def make(k: Int): Inner = new Inner(k)
  def cell[T](v: T): Cell[T] = new Cell(v)
  def hidden(n: Int): Int = new Hidden(n).n2

trait Shelf:
  def label: String
  class Item(val name: String):
    def show: String = label + ":" + name

class Store(val label: String) extends Shelf

@main def innerclasses(): Unit =
  val o = Outer(3)
  val i = new o.Inner(4)
  println(i.sum)
  println(o.make(5).sum)
  println(new o.Empty().three)
  println(o.hidden(6))
  println(o.cell("c").pair)
  val s = Store("s")
  println(new s.Item("a").show)
