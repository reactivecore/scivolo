object Namedvarargs:
  def f(a: Int, xs: String*): String = s"$a ${xs.mkString(",")}"
  def g[T <: AnyRef](first: T, rest: T*): Int = rest.length
  def h(a: Int = 7, b: String = "b", xs: String*): String = s"$a $b [${xs.mkString(",")}]"
  def say(s: String): String = { println(s"eval $s"); s }
  def num(n: Int): Int = { println(s"eval $n"); n }

  def main(args: Array[String]): Unit =
    println(f(a = 1, xs = "x", "y", "z"))
    println(f(1, xs = "x", "y"))
    println(f(a = 2).trim)
    println(g(first = "a", rest = "b", "c"))
    println(f(xs = "x", a = 1))
    println(f(xs = say("y"), a = num(2)))
    println(h(xs = say("z")))
    println(h(b = say("v"), xs = say("u"), a = num(4)))
    println(h())
    println(g(rest = say("r"), first = say("q")))
