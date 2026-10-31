//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Terms.*

object Prefixes:
  val a = termTypeStructure(new Object)
  val b = termTypeStructure(new java.lang.StringBuilder)
  val c = termTypeStructure(new scala.collection.mutable.ArrayBuffer[Int])
  val d = termTypeStructure(Some(1))
  val e = termTypeStructure(??? : Option[Int])
