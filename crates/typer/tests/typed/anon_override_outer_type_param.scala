object AnonOverrideOuterTypeParam {
  abstract class L[+T] {
    def app[E >: T](that: => L[E]): L[E]
    def app2[E >: T](that: L[E]): L[E]
  }
  class P[+T] extends L[T] {
    def app[E >: T](that: => L[E]): L[E] = that
    def app2[E >: T](that: L[E]): L[E] = that
    def m1[E >: T]: L[E] =
      new P[E] {
        override def app[E2 >: E](that2: => L[E2]): L[E2] = that2
      }
    def m2[E >: T]: L[E] =
      new P[E] {
        override def app2[E2 >: E](that2: L[E2]): L[E2] = that2
      }
  }
}
