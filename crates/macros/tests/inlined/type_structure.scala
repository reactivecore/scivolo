//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Terms.*

object TypeStructure:
  def f(): Unit =
    class C
    trait D
    val a = typeStructure[1]
    val b = typeStructure["a" | true]
    val c = typeStructure[C & D]
    val d = typeStructure[C { type X = 2L }]
    val e = typeStructure['c' | 1.5 | 2.5f]
