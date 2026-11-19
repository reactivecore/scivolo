import language.experimental.captureChecking

class A
class B
class Label[T]

case class C(a: A^, b: B^{a})
case class V(a: A^, cs: Label[Int]^{a}*)

@main def casecaptures(): Unit =
  val a = new A
  val c = C(a, new B)
  println(c.a eq a)
  val v = V(a, new Label[Int], new Label[Int])
  println(v.cs.length)
