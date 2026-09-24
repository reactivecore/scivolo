//> using scala 3.9.0

case class Point(x: Int, y: Int)

class Config(path: String):
  lazy val size: Int = path.length * 2

def parse(s: String): Int =
  try s.toInt
  catch
    case e: NumberFormatException => -1
    case e: IllegalStateException => -2
  finally println("parsed")

def classify(n: Int): String = n match
  case 0 => "zero"
  case 1 | 2 => "small"
  case _ => "big"

def adder(k: Int): Int => Int = x => x + k

@main def features(): Unit =
  val xs = List(1, 2, 3).map(_ * 2)
  println(s"xs=$xs sum=${xs.sum}")
  println(Point(1, 2) == Point(1, 2))
  println(Config("abc").size)
  println(parse("12") + parse("x"))
  println(classify(1) + adder(3)(4))
