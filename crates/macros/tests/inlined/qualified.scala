//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Terms.*

object Qualified:
  val a = code {
    class C {
      private[C] def d = 0
      protected[C] def g = 0
      private[this] def c = 0
    }
    ()
  }
  val b = code { @deprecated type T = Int; val x: T = 1; x }
