//> using scala 3.9.0
// Local classes using their enclosing instance, inner classes created through prefixes and
// subclasses of inner classes.

trait Greeter {
  def name: String = "greeter"
  def greet(): String = {
    val suffix = "!"
    object Once {
      val text = "hello " + name + suffix
    }
    Once.text
  }
}
class Polite extends Greeter { override def name = "polite" }

class Counter(start: Int) {
  def total = start * 2
  def make(step: Int) = {
    class Step(by: Int) { def next = total + by + step; def owner = Counter.this.total }
    new Step(1).next + new Step(2).owner
  }
  def viaLocal(k: Int) = {
    def scaled(i: Int) = i * start + k
    class Use(j: Int) { def get = scaled(j); val first = scaled(0) }
    new Use(3).get + new Use(4).first
  }
  val callbacks = new scala.runtime.AbstractFunction0[Int] { def apply() = total + 1 }
}

class Seed(k: Int) { val v = { class Grow { def g = k + 1 }; new Grow().g } }

class Shelf(label: String) {
  class Item(val n: Int) { def show = label + ":" + n }
  class Big extends Item(100)
  private class Small extends Item(1) { def tiny = show + "!" }
  def small = new Small().tiny
  def odd(k: Int) = new Item(k) { override def show = "odd " + label + n }
  class Box {
    def make = new Item(7)
    class Inside { def make2 = new Item(8) }
  }
  trait Tagged { def tag = "tag " + label }
}
object Shelves { val main = new Shelf("main") }
class Extra extends Shelves.main.Item(5)

class Base(f: Int => Int) { def result = f(3) }
class Holder(z: Int) {
  class Base2(f: Int => Int) { def result = f(3) }
  class Child(x: Int) extends Base2(y => x + y + z)
}

object Test {
  def tagged(s: Shelf) = {
    class T extends s.Tagged
    new T().tag
  }
  def sub(x: Int) = {
    class P(val y: Int) { val px = x + y }
    class Q extends P(2) { def q = px + 1 }
    new Q().q
  }
  def run(): Unit = {
    println(new Polite().greet())
    val c = new Counter(3)
    println(c.make(10))
    println(c.viaLocal(1))
    println(c.callbacks())
    println(new Seed(4).v)
    val s = new Shelf("s")
    println(new s.Big().show)
    println(s.small)
    println(s.odd(3).show)
    val b = new s.Box
    println(b.make.show)
    println(new b.Inside().make2.show)
    println(new s.Item(4) {}.show)
    println(new Extra().show)
    println(tagged(s))
    println(new s.Tagged {}.tag)
    println(sub(5))
    val h = new Holder(2)
    println(new h.Child(2).result)
  }
}

@main def outerclasses(): Unit = Test.run()
