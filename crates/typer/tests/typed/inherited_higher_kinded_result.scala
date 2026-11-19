object InheritedHigherKindedResult {
  trait Bar[+F[_]]
  trait Base[+A]
  trait Sub[A] extends Base[A]
  trait Foo[+F[_]] {
    def bar: Bar[F]
  }
  def makeFoo(barSub: Bar[Sub]): Foo[Base] =
    new Foo[Base] {
      def bar = barSub
    }
  class C(barSub: Bar[Sub]) extends Foo[Base] {
    def bar = barSub
  }
}
