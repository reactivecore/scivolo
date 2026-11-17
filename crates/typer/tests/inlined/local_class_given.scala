import scala.compiletime.summonInline

object LocalClassGiven {
  trait Show[T] { def show(t: T): String }
  given Show[Int] = (t: Int) => s"int $t"

  inline def twice(using n: Int): Int = n * 2

  inline def outside[T](t: T, n: Int): String = {
    class C {
      given Int = n
      def v = summon[Int] + 1
    }
    summonInline[Show[T]].show(t) + new C().v
  }

  inline def inside(n: Int): Int = {
    class D {
      given Int = n
      def v = twice
    }
    new D().v
  }

  def a = outside(1, 41)
  def b = inside(5)
}
