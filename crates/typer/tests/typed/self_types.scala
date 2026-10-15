class Counter { self =>
  var n = 0
  def inc(): Counter = { self.n += 1; self }
  def same = locally(this: self.type)
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
  def greet = "hi " + name + self.name + this.name
}
abstract class Both { self: Named & Greeter =>
  def both = greet + name
}
