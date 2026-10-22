//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Macros.*

object Calls:
  val a = twice(21)
  val b = greet("Bob")
  val c = pair("a")
  val d = listOf(1, 2)
  val e = incremented(41)
  def f(n: Int) = plusOneTwice(n)
  val g = wrapped(1L)
  val h = orNull("x")
