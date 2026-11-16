import language.experimental.captureChecking

class Label[T]
class A
class B

object Lambdas:
  val f: (Label[Int]^{}) => Int = l => 1

object Locals:
  def g(a: A^, b: B^{a}): Int =
    val y = b
    1
  def h(a: A^, b: => B^{a}): Int =
    val y = b
    1
  val a: A^ = A()
  def k(b: Label[B^{a}]^): Int =
    val y = b
    1
