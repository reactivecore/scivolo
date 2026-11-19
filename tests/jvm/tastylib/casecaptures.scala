import language.experimental.captureChecking

class A
class B
class Label[T]

case class C(a: A^, b: B^{a})
case class G[T](a: A^, b: Label[T]^{a})
case class H(a: A^)(val b: Label[Int]^{a})
case class W(a: A^, b: Label[Int]^{a}, c: Label[Int]^{a, b})
class O(val x: A^):
  case class I(a: A^, b: Label[Int]^{a, x})
case class V(a: A^, cs: Label[Int]^{a}*)
