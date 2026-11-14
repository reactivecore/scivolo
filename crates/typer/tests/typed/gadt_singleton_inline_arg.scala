object GadtSingletonInlineArg:
  class Foo[+X]
  enum SUB[-S, +T]:
    case Refl[U]() extends SUB[U, U]

  def f[A, B](sub: A SUB B) =
    given Foo[A] = ???
    sub match
      case SUB.Refl() =>
        val c: Foo[B] = summon[Foo[A]]
        summon[Foo[B]]
