//> using scala 3.9.0
object Paths:
  def get(first: String, more: Array[String]): String = first + more.length
  def get(n: Int): String = n.toString

object Defaults:
  def retry(k: Int): Int =
    def go(x: Int, attempt: Int = 0): Int =
      if attempt < 3 then go(x + k, attempt + 1) else x
    go(1)
  def nested(k: Int): String =
    def pair[A](a: A, b: List[A] = Nil, n: Int = k): String = s"$a$b$n"
    def curried(a: Int)(b: Int = a + 1, c: String = "c"): String = s"$a$b$c"
    def inner(): String = pair("x") + pair(1, n = 5) + curried(2)() + curried(3)(c = "d")
    inner()
  def emptyArray(): String = Paths.get("a", Array.empty)

@main def localdefaults(): Unit =
  println(Defaults.retry(2))
  println(Defaults.nested(7))
  println(Defaults.emptyArray())
