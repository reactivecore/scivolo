object UnboundedConstructorImplicit {
  type G[A]
  given G[H[Int]] = ???

  trait H[X]
  object H {
    given H[Int] = ???
  }

  def take[C[_]](using w: G[C[Int]], f: C[Int]) = ???

  def test = take
}
