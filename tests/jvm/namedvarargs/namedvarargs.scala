object Namedvarargs:
  def f(a: Int, xs: String*): String = s"$a ${xs.mkString(",")}"
  def g[T <: AnyRef](first: T, rest: T*): Int = rest.length

  def main(args: Array[String]): Unit =
    println(f(a = 1, xs = "x", "y", "z"))
    println(f(1, xs = "x", "y"))
    println(f(a = 2))
    println(g(first = "a", rest = "b", "c"))
