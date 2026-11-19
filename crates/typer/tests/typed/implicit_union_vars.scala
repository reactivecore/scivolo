object ImplicitUnionVars {
  object Foo
  type T[A] = A | Any

  def perform[A](using T[A]): A = perform2
  def perform2[A](using T[A]): A = ???

  val intEq = summon[Int =:= (Int | Nothing)]
  val fooSub = summon[Foo.type <:< (Foo.type | Nothing)]
  val oneSuper = summon[(1 | Nothing) <:< 1]
}
