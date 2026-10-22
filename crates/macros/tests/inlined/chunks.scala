//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Chunk

object Chunks:
  def parts(c: Chunk): List[String] = c.parts
  val a = parts("a/b")
