//> using scala 3.9.0
class B
class S extends B
class P1 { def f: S = new S() }
trait T1 { def f: B }
class C1 extends P1 with T1
trait T2 { def f: B }
trait U2 { def f: S = new S() }
class C2 extends T2 with U2
class C3 extends Product { def canEqual(o: Any) = true; def productArity = 0; def productElement(n: Int): Any = 0 }
trait T4 {
  @inline def f(x: Int): Int = x + 1
  @noinline def g(x: Int): Int = x * 2
}
class C4 extends T4 { @inline def k(x: Int): Int = x - 1 }

@main def mixins(): Unit =
  val t1: T1 = C1()
  println(t1.f != null)
  val t2: T2 = C2()
  println(t2.f != null)
  println(C3().productArity)
  println(C3().productIterator.hasNext)
  println(C4().f(1) + C4().g(2) + C4().k(4))
