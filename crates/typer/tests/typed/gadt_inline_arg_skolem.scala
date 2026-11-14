object GadtInlineArgSkolem:
  enum Op[A]:
    case Dup[T]() extends Op[(T, T)]

  def foo[R](f: [A] => Op[A] => R): R = ???

  def test = foo[Unit]([A] => (o: Op[A]) => o match
    case o: Op.Dup[u] =>
      summon[A =:= (u, u)]
      ()
  )
