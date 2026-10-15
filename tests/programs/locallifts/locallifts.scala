//> using scala 3.9.0
// Lambdas and local methods in local classes and local traits, lifted into enclosing ones.

object locallifts {
  def lambdas(x: Int) = {
    class D {
      val k = 3
      def h(ys: List[Int]) = ys.map(y => y + x + k)
      def loc(n: Int) = {
        def sq(m: Int) = m * n
        sq(n)
      }
    }
    val d = new D
    d.h(List(1, 2)).sum + d.loc(2)
  }

  def narrowed(x: Int) = {
    class C1 {
      def f2 = {
        class C2 {
          def f3 = {
            def f4 = x
            def f5 = f4 + 1
            f5
          }
        }
        new C2().f3
      }
    }
    new C1().f2
  }

  def inTrait(x: Int) = {
    def f2 = {
      trait T2 {
        def f3 = {
          def f4 = x
          def f5 = f4
          f5
        }
      }
      class C2 extends T2
      new C2().f3
    }
    f2
  }

  def traitInClass(x: Int) = {
    class T1 {
      def f2 = {
        trait T2 {
          def f3 = {
            def f4 = x
            def f5 = f4
            f5
          }
        }
        class C2 extends T2
        new C2().f3
      }
    }
    class C1 extends T1
    new C1().f2
  }

  def traitInTrait(x: Int) = {
    trait T1 {
      def f2 = {
        trait T2 {
          def f3: Int = {
            def f4 = x
            def f5 = f4
            f5
          }
          def f3a = f3
        }
        class C2 extends T2
        class C3 extends T1
        new C2().f3a + new C3().f6
      }
      def f6 = x
    }
    class C1 extends T1
    new C1().f2
  }

  def main(args: Array[String]): Unit = {
    println(lambdas(5))
    println(narrowed(4))
    println(inTrait(5))
    println(traitInClass(6))
    println(traitInTrait(8))
  }
}
