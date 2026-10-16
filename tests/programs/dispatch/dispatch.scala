//> using scala 3.9.0

trait Shape:
  def area: Int
  def scaled(k: Int): Int = area * k

trait Named:
  def code: Int = 7

abstract class Base(val w: Int) extends Shape:
  def name: Int = 1

final class Rect(w: Int, h: Int) extends Base(w) with Named:
  def area: Int = w * h
  override def name: Int = 2

class Square(s: Int) extends Base(s):
  def area: Int = s * s

object Counter:
  var calls: Int = 0
  def tick(): Int =
    calls += 1
    calls

def classify(x: AnyRef): Int = x match
  case _: Rect  => 10
  case s: Shape => 20 + s.area
  case _        => 30

def run(n: Int): Int =
  val r: Base = Rect(n, 2)
  val s: Shape = Square(n)
  val named = r match
    case n: Named => n.code
    case _        => 0
  r.area + s.scaled(3) + r.name * 100 + r.w + named + classify(r) + classify(s) + classify(Counter) +
    Counter.tick() + Counter.tick()

@main def dispatch(args: String*): Unit =
  println(run(args.headOption.fold(20)(_.toInt)))
