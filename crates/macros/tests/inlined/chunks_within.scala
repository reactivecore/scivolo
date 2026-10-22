//> using options --fresh -classpath tests/macros/lib/macros.jar
package macrolib

object ChunksWithin:
  def parts(c: Chunk): List[String] = c.parts
  val a = parts("a/b")
