trait T

object A:
  class B extends T
  class C extends T
  class D[X]

  given b: B()
  given c: C()
  given d: D[Int]()

object Test:
  import A.{given A.B, given A.D[?]}

  val x1: A.B = summon[A.B]
  val x2: A.D[Int] = summon[A.D[Int]]
  val x3: A.B = b
