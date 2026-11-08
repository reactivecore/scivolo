package geo

import scala.collection.mutable.ArrayBuffer

trait Shape {
  def area: Double
  def describe(): String = s"shape of area $area"
}

class Rect(val w: Double, val h: Double) extends Shape {
  def area: Double = w * h
  override def describe(): String = "rect " + super.describe()
  private[geo] def scaled(k: Double): Rect = new Rect(w * k, h * k)
}

object Rect {
  def square(side: Double): Rect = new Rect(side, side)
}

class Cell[A](var value: A) {
  def swap(a: A): A = {
    val old = value
    value = a
    old
  }
  def into(buffer: ArrayBuffer[A]): Unit = buffer.addOne(value)
}

object Shapes {
  val unit: Shape = Rect.square(1.0)
  def total(shapes: List[Shape]): Double = shapes.map(_.area).sum
  def largest(shapes: List[Shape]): Option[Shape] =
    if (shapes.isEmpty) None else Some(shapes.maxBy(_.area))
  def firstPositive(xs: Array[Int]): Int = {
    var i = 0
    while (i < xs.length) {
      if (xs(i) > 0) return xs(i)
      i += 1
    }
    -1
  }
  def main(args: Array[String]): Unit = println(total(List(unit, new Rect(2.0, 3.0))))
}
