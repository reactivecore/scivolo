package demo

import scala.collection.mutable as m
import scala.util.{Try, Success as Ok, *}

class Printer:
  def print(s: String): Unit = println(s)
  def flush(): Unit = ()

class Logger(p: Printer):
  export p.{print as log, *}
  export scala.math.{max, min}

trait Greeter(val name: String):
  def greet: String = s"Hello, $name"

class Impl extends Greeter("x"), Serializable

case class Point(x: Int, y: Int) derives CanEqual

@main def run(args: String*): Unit =
  val s = s"sum: ${1 + 2} and ${Point(1, 2).x}"
  val f = f"$s%s ${3.14}%.2f"
  val raw = raw"\d+"
  val multi = s"""line $s
    ${args.mkString(",")}"""
  val xs = List(
    1,
    2,
  )
  val p @ Point(a, b) = Point(1, 2): @unchecked
  val lambda = (x: Int) ?=> x
  val pf: PartialFunction[Int, Int] = { case x if x > 0 => x }
  println(xs.map { case x => x + a + b })
  val big = 1_000_000L
  val hex = 0xFF_FF
  val ch = 'c'
  val sym = 'c'.toInt
  val op = xs.foldLeft(0)(_ + _)
  val ascribed = (xs: Seq[Int])
  val splat = List(xs*)
