import language.experimental.captureChecking

class A
class B

case class C(a: A^, b: B^{a})

@main def casecaptures(): Unit =
  val a = new A
  val c = C(a, new B)
  println(c.a eq a)
