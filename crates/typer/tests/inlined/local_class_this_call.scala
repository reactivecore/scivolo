import scala.compiletime.summonInline

object LocalClassThisCall {
  given Int = 1

  object Imported { given Int = 50 }

  inline def secondary: String = {
    class D(val a: Int, val b: String) {
      import Imported.given
      given Long = 5L
      var c = 0L
      def this(s: String) = {
        this(summonInline[Int], s)
        c = summonInline[Long]
      }
      def this(s: String, n: Long)(using i: Int) = {
        this(summonInline[Int] + 1, s)
        c = summonInline[Long] + n
      }
    }
    new D("x").b + new D("y", 1L)(using 7).b
  }

  def a = secondary
}
