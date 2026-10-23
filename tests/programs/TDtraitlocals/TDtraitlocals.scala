object TDtraitlocals {
  def f(x: Int) = {
    trait T1 {
      trait T2 {
        def f3: Int = x
      }
      class C2 extends T2 {
        override def f3 = super.f3 + 1
      }
      val y = new C2().f3 + x
      def f6 = x + y
    }
    class C1 extends T1
    new C1().f6
  }

  def main(args: Array[String]): Unit = {
    println(f(10))
  }
}
