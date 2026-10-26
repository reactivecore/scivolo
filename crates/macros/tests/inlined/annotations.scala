//> using options --fresh -classpath tests/macros/lib/macros.jar -experimental
import macrolib.{labelled, traced}

class Annotated:
  @traced("x") val x: Int = 1 + 2
  @traced("f") def f(n: Int): Int = n * 2
  @traced("g") @traced("h") def g: String = "g"

@labelled class Labelled(val n: Int):
  def twice = n * 2
