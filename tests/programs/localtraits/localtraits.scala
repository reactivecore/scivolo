//> using scala 3.9.0
// Local traits, lambdas and local methods in inner and nested classes, classes nested in local
// classes.

trait Shape { def area: Int }

class Board(size: Int) {
  def cell = size * 2
  def describe(label: String) = {
    trait Named extends Shape {
      def area = cell + label.length
      def twice = Board.this.cell * 2
    }
    class Square extends Named
    new Square().area + new Square().twice
  }
  class Row(n: Int) {
    def doubled(xs: List[Int]) = xs.map(x => x * 2)
    def scaled(xs: List[Int]) = xs.map(x => x * size)
    def plain(k: Int) = { def times(i: Int) = i * k; times(n) }
    def outer(k: Int) = { def times(i: Int) = i * cell + k; times(n) }
    val first = List(n).map(x => x + size)
  }
}

object Shapes {
  class Tile {
    def grow(xs: List[Int]) = xs.map(x => x + 1)
    def times(n: Int) = { def by(i: Int) = i * n; by(3) }
  }

  def make(x: Int, y: String): Int = {
    trait Box {
      def width = x
      def text(n: Int) = y * n
      def fixed = 7
      def both = width + fixed
    }
    class Crate extends Box
    val c = new Crate
    val d = new Box {}
    c.width + c.text(2).length + c.fixed + d.width + d.both
  }

  def kinds(n: Int) = {
    abstract class Kind { def name: String }
    case object Small extends Kind { def name = "small" }
    trait Picker {
      def pick() = describe(Small)
      def describe(k: Kind) = k.name * n
    }
    new Picker {}.pick()
  }

  def chained(k: Int) = {
    class Cell { def x = k }
    trait Reader { def read = new Cell().x }
    trait Base { def m = 1 }
    trait Over extends Base { override def m = k }
    class Both extends Over with Reader
    val b = new Both
    b.m + b.read
  }

  def counted(step: Int) = {
    var count = 0
    trait Counter { def bump() = count += step }
    class Ticks extends Counter
    val t = new Ticks
    t.bump()
    t.bump()
    count
  }

  def recursive(k: Int) = {
    trait Fact { def fact(n: Int): Int = if n <= 1 then k else n * fact(n - 1) }
    new Fact {}.fact(4)
  }

  def overriding(k: Int) = {
    trait Base { def m = k }
    class Plus extends Base { override def m = super.m + 1 }
    new Plus().m
  }

  def twoClasses(a: Int, b: Int) = {
    class First { def get = a }
    class Second { def get = b * 10 }
    new First().get + new Second().get
  }

  def parenless(k: Int) = {
    trait Value { def v = k * 2 }
    new Value {}.v
  }

  def fromClass(k: Int) = {
    trait Source { def m = k }
    class User { def use(s: Source) = s.m + 1 }
    new User().use(new Source {})
  }

  def inMethods(k: Int) = {
    class Holder {
      def m = { class Local { def x = k * 3 }; new Local().x }
      def r = new Runnable { def run() = println(k + 1) }
      class Member { def r = new Runnable { def run() = println(k + 2) } }
    }
    val h = new Holder
    h.r.run()
    new h.Member().r.run()
    h.m
  }

  def nested(k: Int, s: String) = {
    class Outer {
      class Inner {
        def m = k + 1
        class Deeper { def n = s * k }
      }
      def g = {
        val i = new Inner()
        i.m + new i.Deeper().n.length
      }
    }
    new Outer().g
  }
}

object localtraits {
  def main(args: Array[String]): Unit = {
    val b = new Board(3)
    println(b.describe("ab"))
    val r = new b.Row(2)
    println(r.doubled(List(1, 2)))
    println(r.scaled(List(1, 2)))
    println(r.plain(5))
    println(r.outer(5))
    println(r.first)
    val t = new Shapes.Tile
    println(t.grow(List(1)))
    println(t.times(4))
    println(Shapes.make(3, "ab"))
    println(Shapes.kinds(2))
    println(Shapes.chained(5))
    println(Shapes.counted(3))
    println(Shapes.recursive(2))
    println(Shapes.overriding(4))
    println(Shapes.twoClasses(1, 2))
    println(Shapes.parenless(6))
    println(Shapes.fromClass(7))
    println(Shapes.inMethods(8))
    println(Shapes.nested(2, "xy"))
  }
}
