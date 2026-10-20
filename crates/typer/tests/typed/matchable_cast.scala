object Test {
  def g[T <: Matchable](x: T): T = x
  def f[T](x: T): Matchable = g(x)
  def main(args: Array[String]): Unit = println(f(1))
}
