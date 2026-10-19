import scala.reflect.Selectable.reflectiveSelectable

class Point {
  val x: Int = 3
  def name: String = "p"
  def scaled(k: Int, unit: String): String = s"${x * k}$unit"
  def +(k: Int): Int = x + k
}

object structural:
  extension (p: Point) def name: String = "extension"

  def show(p: { val x: Int; def name: String; def scaled(k: Int, unit: String): String }): String =
    p.name + p.x + " " + p.scaled(2, "cm")

  def plus(p: { def +(k: Int): Int }): Int = p + 4

  def bump(c: { def n: Int; def n_=(v: Int): Unit }): Int =
    c.n = c.n + 1
    c.n

  class Counter { var n = 10 }

  def main(args: Array[String]): Unit =
    println(show(new Point))
    println(plus(new Point))
    println(bump(new Counter))
