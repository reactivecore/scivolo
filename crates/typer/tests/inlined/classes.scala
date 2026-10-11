trait T {
  inline def tw: Int = 2
}

class C(val n: Int) {
  def base: Int = n * 10
  inline def plus(m: Int): Int = n + m
  inline def twice: Int = base + base
  inline def self: C = this
  def useInside(k: Int): Int = plus(k)
}

class D(val k: Int) extends T {
  val f: Int = k + 1
  inline def both(x: Int): Int = f + k + x
  inline def unused: Int = 3
  def inner: Int = both(1) + this.both(2) + tw
}

case class P(a: Int, b: Int) {
  inline def sum: Int = a + b
}

object Test {
  inline def inc(x: Int): Int = x + 1
  def mk(): C = { println("mk"); C(1) }
  def mkD(): D = { println("mkD"); D(1) }
  def main(args: Array[String]): Unit = {
    val c = C(2)
    println(c.plus(3))
    println(mk().plus(4))
    println(c.twice)
    println(mk().self.n)
    println(c.useInside(5))
    val d = D(4)
    println(d.both(5))
    println(mkD().unused)
    println(d.tw)
    println(P(1, 2).sum)
    val p = P(3, 4)
    println(p.sum)
    println(d.inner)
    println(inc(c.n))
  }
}
