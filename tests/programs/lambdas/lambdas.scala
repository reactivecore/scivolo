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

@main def lambdas(): Unit =
  println(L.suffix())
  println(L.captured("+"))
  println(L.constant())
  println(L.joined())
  println(L.field())
  println(L.nested("."))
  L.printed()
