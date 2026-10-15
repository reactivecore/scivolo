//> using scala 3.9.0
// Partial functions from case lambdas and lambdas, expanded to anonymous classes.

object Pfs {
  val positive: PartialFunction[Int, String] = { case x if x > 0 => "pos " + x }
  val pairs: PartialFunction[(Int, String), Int] = { case (n, s) if s.nonEmpty => n + s.length }
  val total: PartialFunction[Int, Int] = x => x * 2
  val matched: PartialFunction[Any, String] = x => x match {
    case s: String => s
    case n: Int if n > 1 => n.toString
  }
}

object Test {
  def run(): Unit = {
    println(Pfs.positive.isDefinedAt(1))
    println(Pfs.positive.isDefinedAt(-1))
    println(Pfs.positive(3))
    println(Pfs.pairs((2, "ab")))
    println(Pfs.pairs.isDefinedAt((2, "")))
    println(Pfs.total.isDefinedAt(5))
    println(Pfs.total(5))
    println(Pfs.matched.isDefinedAt(1.0))
    println(Pfs.matched("s"))
    println(List(1, -2, 3).collect { case x if x > 1 => x * 10 })
    println(List(1, -2, 3).collect(Pfs.positive))
    println(Pfs.positive.lift(-1))
  }
}

@main def partialfuns(): Unit = Test.run()
