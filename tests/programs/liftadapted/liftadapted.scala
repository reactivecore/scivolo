object liftadapted {
  def f(a: Array[String], n: Int): Int = {
    def g(): Int = n
    def k(): Int = g()
    val h = (i: Int) => a(i).charAt(n)
    class C { def inner(): Int = g() }
    k() + h(0) + new C().inner()
  }

  def m(a: Array[String], n: Int): Int = {
    def g(): Int = n
    def k(): Int = g()
    val h = (i: Int) => a(i).charAt(n)
    def outer(): Int = {
      def inner(): Int = g()
      inner()
    }
    k() + h(0) + outer()
  }

  def p(a: Array[String], n: Int): Int = {
    def g(): Int = n
    def outer(): Int = {
      val h = (i: Int) => a(i).charAt(g())
      def inner(): Int = g() + h(0)
      inner()
    }
    outer()
  }

  def main(args: Array[String]): Unit = {
    println(f(Array("ab", "cd"), 1))
    println(m(Array("ab", "cd"), 1))
    println(p(Array("ab", "cd"), 1))
  }
}
