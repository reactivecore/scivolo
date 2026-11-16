//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Searches.annotationsOf

object Test:
  val a = annotationsOf[macrolib.Retired]
  val b = annotationsOf[macrolib.Retiring]
