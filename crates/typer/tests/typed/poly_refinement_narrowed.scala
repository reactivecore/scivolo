object PolyRefinementNarrowed {
  trait Poly {
    def f[T]: String
  }
  def p: Poly { def f[T]: "hello" } = ???
  def q: "hello" = p.f[Int]
}
