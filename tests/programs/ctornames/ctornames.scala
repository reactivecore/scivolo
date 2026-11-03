//> using scala 3.9.0
// Anonymous classes and lambdas of constructors named before those of methods; a lazy local
// value initialized from an erased result.

trait CNW { def w: Int }

trait CNTrait {
  def a(x: Int): CNW = new CNW { def w = x }
  val b: CNW = new CNW { def w = 2 }
}

class CNClass(strings: Seq[(String, Int)]) {
  def a(x: Int): CNW = new CNW { def w = x }
  val b: CNW = new CNW { def w = 3 }
  val (min, max) = {
    val all = strings.map(_._1(0))
    (all.min, all.max)
  }
}

object ctornames extends CNTrait {
  def lazily(g: Int => Iterator[String]): Int = {
    lazy val it = g(1)
    it.size
  }

  def main(args: Array[String]): Unit = {
    val c = new CNClass(Seq(("ab", 1), ("cd", 2)))
    println(a(1).w + b.w + c.a(4).w + c.b.w)
    println(s"${c.min} ${c.max}")
    println(lazily(i => Iterator("x", "y")))
  }
}
