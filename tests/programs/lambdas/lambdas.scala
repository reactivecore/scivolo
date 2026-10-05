//> using scala 3.9.0
object L:
  def twice(f: String => String, x: String): String = f(f(x))
  def run(f: () => String): String = f()
  def both(f: (String, Int) => String): String = f("n", 2)
  def suffix(): String = twice(x => x + "!", "a")
  def captured(k: String): String = twice(x => x + k, "b")
  def constant(): String = run(() => "c")
  def joined(): String = both((s, i) => s + i)
  val base = "base"
  def field(): String = twice(x => x + base, "d")
  def nested(k: String): String = twice(x => twice(y => y + k, x), "e")
  def each(f: String => Unit): Unit = f("u")
  def printed(): Unit = each(s => println(s))
  def go(k: String): String = twice(x => x + k, "l")

object M:
  def twice(f: String => String, x: String): String = f(f(x))
  def adapt(f: (Int, String) => String): String = f(1, "s")
  def withK(k: String): String = adapt((i, s) => s + i + k)
  def go(k: String): String = twice(x => x + k, "m")
  def order(a: String, b: String, c: String): String = twice(x => x + c + a + b, "o")
  def deep(p: String, q: String): String = twice(x => twice(y => y + q, x) + p, "d")

@main def lambdas(): Unit =
  println(L.suffix())
  println(L.captured("+"))
  println(L.constant())
  println(L.joined())
  println(L.field())
  println(L.nested("."))
  L.printed()
  println(L.go("1") + M.go("2") + M.withK("3"))
  println(M.order("a", "b", "c") + M.deep("p", "q"))
