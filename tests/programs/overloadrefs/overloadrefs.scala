// Overloaded methods named without arguments: the alternative chosen by the expected type.

trait Shape
trait Circle extends Shape
trait Square extends Shape

object Refs {
  def v: String = "v: String"
  def v[T]: List[T] = Nil

  def f(x: String): String = s"f(String) $x"
  def f[T >: Null](x: T): String = s"f[T] $x"

  def g(): String = "g()"
  def g(x: Int): String = s"g(Int) $x"

  def m(x: Float): String = s"m(Float) $x"
  def m(x: Float, y: Int): String = s"m(Float, Int) $x $y"

  def empty[E]: List[E] = { println("empty[E]"); Nil }
  def empty(implicit a: Int): Any = { println("empty(implicit)"); a }

  def on(f: Square => String): String = f(new Square {})
  def draw(s: Shape): String = "draw(Shape)"
  def draw(s: Square): String = "draw(Square)"
  def draw(s: Circle): String = "draw(Circle)"

  def run(n: Int*): String = s"run(Int*) ${n.sum}"
}

@main def overloadrefs(): Unit = {
  import Refs.*
  val v1: String = v
  println(v1)
  val x4: String => String = f
  println(x4("a"))
  val x5: String => Any = f
  println(x5("b"))
  val y1: Int => String = g
  println(y1(1))
  val y2: () => String = g
  println(y2())
  val y3: Function1[? >: Float, String] = m
  println(y3(2f))
  empty
  println(on(draw))
  val r: Seq[Int] => String = run
  println(r(Seq(1, 2, 3)))
}
