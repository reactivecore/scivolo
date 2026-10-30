//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Warnings.*

object Warned:
  val a = warned(1)
