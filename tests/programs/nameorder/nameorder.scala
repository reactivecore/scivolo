class Fd(val value: Int) extends AnyVal

object Binders {
  def id(a: Any): Any = a
  def f(a: Any): Int = id(a) match { case s: String => s.length; case _ => 2 }
  def twice(xs: Array[Int]): Array[Int] = xs.map(x => x * 2)
  def local(n: Int): Int = {
    def step(i: Int): Int = if (i > 0) step(i - 1) + n else 0
    step(3)
  }
  class Inner {
    def h(a: Any): Int = id(a) match { case s: String => s.length; case _ => 4 }
  }
}

class Binders(n: Int) {
  private val (a, b) = (new Fd(n), new Fd(n + 1))
  def k(a: Any): Int = Binders.id(a) match { case s: String => s.length; case _ => 5 }
  def plus(xs: Array[Int]): Array[Int] = xs.map(x => x + n)
  def local(m: Int): Int = {
    def step(i: Int): Int = if (i > 0) step(i - 1) + m else 0
    step(2)
  }
  def sum: Int = a.value + b.value
}

object nameorder {
  def main(args: Array[String]): Unit = {
    val b = new Binders(1)
    println(Binders.f("abc") + Binders.f(1) + new Binders.Inner().h("x") + b.k(2))
    println(Binders.twice(Array(1, 2)).mkString(",") + " " + b.plus(Array(1, 2)).mkString(","))
    println(Binders.local(2) + b.local(3) + b.sum)
  }
}
