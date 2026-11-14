object ArgSkolems:
  enum Op[A]:
    case Dup[T]() extends Op[(T, T)]

  inline def same[T](x: T): x.type = x
  inline def sm[T](using x: T): x.type = x
  def foo[R](f: [A] => Op[A] => R): R = ???

  def applied = same(Option(1))
  def block = same[Option[Int]]({ val y = 1; Option(y) })
  def test = foo[Unit]([A] => (o: Op[A]) => o match
    case o: Op.Dup[u] =>
      sm[A =:= (u, u)]
      ()
  )
