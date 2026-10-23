//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.LhReflect.*

object LhReflectUse:
  val a = isOne(1)
  val b = isOne(2)
  val c = size("abc")
  val d = found[Ordering[Int]]
  val e = found[Ordering[Object]]
  val f = isA('b')
