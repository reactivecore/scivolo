//> using options --fresh -classpath tests/macros/lib/macros.jar -experimental
import macrolib.traced

class Annotated:
  @traced("x") val x: Int = 1 + 2
  @traced("f") def f(n: Int): Int = n * 2
  @traced("g") @traced("h") def g: String = "g"
