//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Source.*

object Positions:
  val a = line
  val b = location
  val c = name
  val d = fullName
  val e = enclosing
  def f(n: Int) = text(n + 1)
  val g = text(List(1, 2))
  object Inner:
    def h = enclosing
