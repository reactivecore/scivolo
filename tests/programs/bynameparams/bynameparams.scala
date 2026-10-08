//> using scala 3.9.0
object B:
  def twice(x: => Int): Int = x + x
  def unless(c: Boolean)(body: => Unit): Unit = if !c then body
  def orElse(s: => String, d: => String): String = if s.isEmpty then d else s
  def pass(x: => Int): Int = twice(x)
  def typed(x: => Int): Int = twice(x: Int)
  def lazily(x: => String): () => String = () => x
  def lazyUnit(x: => Unit): () => Unit = () => x
  def unit0(u: () => Unit): () => Unit = () => u()

class C(x: => Int):
  def get: Int = x + x
  def t: Int = B.twice(x)

object E:
  val of: String => String = s => s + "?"
  def objVal(): List[String] = List("a").map(s => of(s))

class K(f: String => String, val g: String => String):
  def m: List[String] = List("a").map(s => f(s))
  def n: List[String] = List("a").map(s => g(s))

@main def bynameparams(): Unit =
  var n = 0
  println(B.twice({ n += 1; n }))
  B.unless(false)(println("ran"))
  B.unless(true)(println("skipped"))
  println(B.orElse("", "dflt"))
  println(B.pass(21))
  println(B.typed(5))
  println(B.lazily("later")())
  B.lazyUnit(println("unit"))()
  B.unit0(() => println("u0"))()
  val inc: Int => Int = _ + 1
  println(List(1, 2).map(x => inc(x)))
  val up: String => String = s => s + "!"
  println(List("a", "b").map(s => up(s)))
  val c = C({ n += 1; n })
  println(c.get)
  println(c.t)
  println(E.objVal())
  val k = K(s => s + "f", s => s + "g")
  println(k.m)
  println(k.n)
