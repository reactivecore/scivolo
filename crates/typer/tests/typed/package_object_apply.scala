package pp {
  trait Printer { def apply[T](x: T): String = x.toString }
  package object printer extends Printer
  package object adder {
    def apply(i: Int): Int = i + 1
    def apply(s: String): String = s + "!"
  }
}
package qq {
  import pp.printer
  object PackageObjectApply {
    val a = printer(1)
    val b = pp.printer[String]("x")
    val c = pp.adder(1)
    val d = pp.adder("s")
  }
}
