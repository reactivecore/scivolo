//> using scala 3.9.0
object B:
  def twice(x: => Int): Int = x + x
  def unless(c: Boolean)(body: => Unit): Unit = if !c then body
  def orElse(s: => String, d: => String): String = if s.isEmpty then d else s
  def pass(x: => Int): Int = twice(x)
  def lazily(x: => String): () => String = () => x

@main def bynameparams(): Unit =
  var n = 0
  println(B.twice({ n += 1; n }))
  B.unless(false)(println("ran"))
  B.unless(true)(println("skipped"))
  println(B.orElse("", "dflt"))
  println(B.pass(21))
  println(B.lazily("later")())
  val inc: Int => Int = _ + 1
  println(List(1, 2).map(x => inc(x)))
  val up: String => String = s => s + "!"
  println(List("a", "b").map(s => up(s)))
