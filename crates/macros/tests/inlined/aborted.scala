//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Checks.*

object Aborted:
  val e = aborted(5)
