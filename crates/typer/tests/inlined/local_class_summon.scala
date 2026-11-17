import scala.compiletime.{summonFrom, summonInline}

object LocalClassSummon {
  trait G { given Int = 3 }
  trait H { given Long = 4L }

  inline def own(n: Int): Int = {
    class D {
      given Int = n
      val x = summonInline[Int]
      def m = { given Int = 9; summonInline[Int] }
    }
    new D().x + new D().m
  }

  inline def inherited: Int = {
    class D extends G { def m = summonInline[Int] }
    new D().m
  }

  inline def selfType: Long = {
    trait E { self: H => def m = summonInline[Long] }
    (new E with H {}).m
  }

  inline def nested(n: Int): Int = {
    given Int = 1
    class D {
      given Int = n
      class E { def m = summonInline[Int] }
      def e = new E
    }
    new D().e.m
  }

  inline def from(n: Int): Int = {
    object D {
      given Int = n
      def m = summonFrom { case i: Int => i + 1 }
    }
    D.m
  }

  class Base(val x: Int)

  inline def superArg: Int = {
    given Int = 1
    class D extends Base(summonInline[Int]) with G
    new D().x
  }

  inline def superParam: Int = {
    given Int = 1
    class D(using i: Int) extends Base(summonInline[Int]) with G
    new D(using 2).x
  }

  def a = own(5)
  def b = inherited
  def c = selfType
  def d = nested(7)
  def e = from(8)
  def f = superArg
  def g = superParam
}
