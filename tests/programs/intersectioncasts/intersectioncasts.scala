//> using scala 3.9.0

class A:
  def a: Int = 1
trait D extends A:
  def d: Int = 2
class B extends A with D

inline def as[T](x: Any): T = x.asInstanceOf[T]

def direct(x: Any): Int =
  val y = x.asInstanceOf[D & A]
  y.a + y.d

def inlined(x: Any): Int =
  val y = as[D & A](x)
  y.a + y.d

def reversed(x: Any): Int =
  val y = x.asInstanceOf[A & D]
  y.d

@main def intersectioncasts =
  println(direct(B()))
  println(inlined(B()))
  println(reversed(B()))
  try println(direct(A()))
  catch case _: ClassCastException => println("cce")
