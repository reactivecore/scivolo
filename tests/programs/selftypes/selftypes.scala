class Counter { self =>
  var n = 0
  def inc(): Counter = { self.n += 1; self }
  class Probe { def read = self.n }
}

class Box[T](val v: T) { outer =>
  def get: T = outer.v
}

class Holder(val x: Int) extends Helpers
abstract class Helpers:
  self: Holder =>
  def f = x
  def g = self.x + 1

class Base { def foo = 1 }
trait UsesBase { this: Base =>
  def t = foo + this.foo
}

trait Named { def name: String }
trait Greeter { self: Named =>
  def greet = "hi " + name + " " + self.name + " " + this.name
}
class Person extends Named with Greeter { def name = "p" }

abstract class Both { self: Named & Greeter =>
  def both = greet + "/" + name
}
class Robot extends Both with Named with Greeter { def name = "r" }

object Mixed extends Base with UsesBase

object selftypes {
  def main(args: Array[String]): Unit = {
    val c = new Counter
    c.inc().inc()
    println(new c.Probe().read)
    println(Box("v").get)
    println(Holder(3).f + Holder(4).g)
    println(Mixed.t)
    println(Person().greet)
    println(Robot().both)
  }
}
