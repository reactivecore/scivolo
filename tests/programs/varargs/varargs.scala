//> using scala 3.9.0
object V:
  def hashed(a: String, b: String): Int = java.util.Objects.hash(a, b)
  def out(s: String, t: String): Unit = System.out.printf("%s=%s%n", s, t)
  def count(xs: String*): Int = xs.length
  def counted(): Int = count("a", "b", "c")
  def none(): Int = count()
  def sum(xs: Int*): Int = xs.length

@main def varargs(): Unit =
  println(V.hashed("a", "b"))
  V.out("x", "y")
  println(V.counted())
  println(V.none())
  println(V.sum(1, 2))
