object LotOuter:
  val k = 1
  object Inner:
    val m = 2
    def f = List(1).map(_ + k)
    def g = List(1).map(_ + m)
    def h = List(1).map(x => x + k + m)
    def o = List(1).map(_ + LotOuter.this.k + m)
    def i = List(1).map(_ + this.m + LotOuter.this.k)
    object Deeper:
      def d = List(1).map(_ * k)
      def e = List(1).map(x => LotOuter.this.k + Inner.this.m + x)
      def p = List(1).map(x => LotOuter.this.k + List(x).map(y => Inner.this.m + y).head)
      def q = List(1).map(x => Inner.this.m + List(x).map(y => LotOuter.this.k + y).head)
  class Member:
    def c = List(1).map(_ - k)
    def n = List(1).map(_ - LotOuter.this.k)

object liftouterthis:
  def main(args: Array[String]): Unit =
    println(LotOuter.Inner.f)
    println(LotOuter.Inner.g)
    println(LotOuter.Inner.h)
    println(LotOuter.Inner.o)
    println(LotOuter.Inner.i)
    println(LotOuter.Inner.Deeper.d)
    println(LotOuter.Inner.Deeper.e)
    println(LotOuter.Inner.Deeper.p)
    println(LotOuter.Inner.Deeper.q)
    println(LotOuter.Member().c)
    println(LotOuter.Member().n)
