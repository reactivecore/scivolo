import language.experimental.captureChecking

class A
class Label[T]
class C(val a: A^, val b: Label[Int]^{a})
class D(a: A^, val b: Label[Int]^{a})
class E(val a: A^, b: Label[Int]^{a}):
  def g = b
class F[T](val a: A^)(val b: Label[T]^{a}, val c: Label[T]^{a, b})
class H(val a: A^):
  class I(val b: Label[Int]^{a}, val c: Label[Int]^{b})
