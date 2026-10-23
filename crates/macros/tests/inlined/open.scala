//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Terms.*

object Open:
  val a = open { val x: Int = 1; x + x }
  val b = open { val x: Int = 1; 3 }
  val c = open { def f(n: Int): Int = if n > 0 then f(n - 1) else n; f(2) }
  val d = open(4)
