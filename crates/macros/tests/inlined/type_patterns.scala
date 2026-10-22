//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.TypePatterns.*

object TypePatterns:
  val a = kind[List[Int]]
  val b = kind[List[String]]
  val c = kind[Map[Int, String]]
  val d = kind[Int]
  val e = kind[String]
  val f = kind[Nil.type]
  val g = emptyOf[Option[Long]]
  val h = emptyOf[String]
  val i = kind[List[Int] & Serializable]
