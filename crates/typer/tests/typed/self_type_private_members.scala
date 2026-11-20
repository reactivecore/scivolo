object test1:
  trait Foo[A]
  trait Baz[A]:
    trait Bar:
      this: Foo[A] =>
      def bar(a: A): Unit

object test2:
  trait Foo:
    private var f = "abc"
  trait Baz:
    trait Bam:
      val f = 0
    trait Bar extends Bam:
      this: Foo =>
      val g = f
      val g1: Int = g
