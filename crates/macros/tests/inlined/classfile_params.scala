//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Searches.paramsOf

object Test:
  val a = paramsOf[macrolib.JavaParams]("pick")
