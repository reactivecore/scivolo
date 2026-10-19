//> using scala 3.9.0
object G:
  def twice(f: (=> Int) => Int): Int = f({ println("eval"); 21 })
  def sum(xs: Int*): Int = xs.sum
  def tagged(xs: Any*)(sep: String = "-"): String = xs.mkString(sep)

@main def smallgroups(): Unit =
  println(G.twice(x => x + x))
  val arr = Array(1, 2, 3)
  println(G.sum(arr*))
  println(G.tagged(1, 2, 3)())
  println(G.tagged(arr*)("+"))
