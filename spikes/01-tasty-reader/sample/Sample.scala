//> using scala 3.9.0

package sample

trait Shape:
  def area: Double

case class Circle(r: Double) extends Shape:
  def area: Double = math.Pi * r * r

case class Rect(w: Double, h: Double) extends Shape:
  def area: Double = w * h

trait Show[A]:
  def show(a: A): String

given Show[Int] with
  def show(a: Int): String = s"Int($a)"

def describe[A](a: A)(using s: Show[A]): String = s.show(a)

extension (s: Shape) def scaled(k: Double): Double = s.area * k

inline def twice(inline x: Int): Int = x + x

transparent inline def pick(inline b: Boolean): Any = if b then 1 else "one"

object Main:
  def classify(s: Shape): String = s match
    case Circle(r) if r > 10 => "big circle"
    case Circle(_)           => "circle"
    case Rect(w, h) if w == h => "square"
    case _: Rect             => "rect"

  def main(args: Array[String]): Unit =
    val shapes = List(Circle(1), Rect(2, 2), Rect(2, 3))
    val inc = (x: Int) => x + 1
    val pairs = for
      s <- shapes
      k <- List(1, 2)
      if k > 1
    yield (classify(s), s.scaled(k))
    println(describe(42))
    println(twice(inc(20)))
    println(pick(true))
    println(s"pairs=${pairs.mkString(",")} total=${shapes.map(_.area).sum}%")
