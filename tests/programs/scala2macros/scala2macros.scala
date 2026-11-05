object scala2macros {
  object A {
    def f(x: Int): Int = macro ???
    inline def f(x: Int): Int = x + 1
  }
  class B {
    def g: String = macro ???
    def g: String = "g"
  }
  def main(args: Array[String]): Unit = {
    println(A.f(41))
    println(new B().g)
  }
}
