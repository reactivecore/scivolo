//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Checks.*

object Checks:
  val n = 3
  val a = positive(1)
  val b = positive(-2)
  val c = positive(n)
  val d = flagged(4)
