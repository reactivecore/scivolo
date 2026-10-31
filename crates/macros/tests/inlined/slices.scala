//> using options --fresh -classpath tests/macros/lib/macros.jar -experimental
import macrolib.Slices.*

object SlicesTest:
  def f(): Int = refinedXs.sum

  def g[T](): String =
    type U = "abc"
    typeArgPos[String] + typeArgPos[U] + typeArgPos[T] + typeArgPos[List[Int]]
