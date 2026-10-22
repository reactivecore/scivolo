//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Checks.*

object Types:
  val a = positive(1)
  val e = sameType[Int, Int]
  val f = sameType[Int, Long]
  val g = subType[Nil.type, List[Int]]
  val h = subType[List[Int], Seq[Any]]
  val i = subType[Seq[Int], List[Int]]
  val j = isInt(1 + 1)
  val k = isInt("s")
  val l = asInt(5)
  val m = widened(7)
