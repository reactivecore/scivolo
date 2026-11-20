object t1:
  class C
  class A:
    val c: B.c.type = B.c
  object B:
    val c = new C
    val a = new A:
      def m = c

object t2:
  class C[T]:
    type TT = T
  object O:
    type TT = String
    class D extends C[TT]:
      def n(x: TT) = x
