//> using scala 3.9.0
// Protected members of Scala classes used from classes in other packages that do not derive
// from their owners: getters, setters, generic members, objects, sub-packages.

package a {
  class Base {
    protected var count: Int = 0
    protected def bump(n: Int): Int = { count += n; count }
    protected val label: String = "base"
    protected def wide: Int = 7
  }
  class Box[T](t: T) {
    protected def put(x: T): T = x
    protected def held: T = t
  }
  package c {
    class Sub extends a.Base {
      class Inner {
        def run(): String = {
          count = 5
          count += 1
          s"${bump(2)} $label $count $wide"
        }
      }
    }
  }
}

package b {
  class Sub extends a.Base {
    class Inner {
      def run(): String = {
        count = 5
        count += 1
        s"${bump(2)} $label $count"
      }
    }
    def viaLambda: () => Int = () => bump(1)
  }
  object O extends a.Base {
    object In { def f: Int = bump(1) }
    class K { def g: Int = { count = 3; bump(2) + count } }
    def h: () => Int = () => bump(0)
    def viaIn: Int = In.f
  }
  class S extends a.Base {
    object Helper { def f: Int = bump(4) }
    def lam: Int => Int = x => { class L { def v = bump(x) }; new L().v }
  }
  class IntBox extends a.Box[Int](4) {
    class Get { def both: Int = put(3) + held }
  }
}

object protectedpackages {
  def main(args: Array[String]): Unit = {
    val s = new b.Sub
    println(new s.Inner().run())
    println(s.viaLambda())
    val t = new a.c.Sub
    println(new t.Inner().run())
    println(b.O.viaIn + new b.O.K().g + b.O.h())
    val u = new b.S
    println(u.Helper.f + u.lam(5))
    val box = new b.IntBox
    println(new box.Get().both)
  }
}
