package basics

class Box[A](val value: A) {
  var count: Int = 0
  def map[B](f: A => B): Box[B] = new Box(f(value))
  def get: A = value
}

object Box {
  def of[A](a: A): Box[A] = new Box(a)
  val zero: Int = 0
}

trait Shape {
  def area: Double
  def name: String = "shape"
}

class Square(side: Double) extends Shape {
  def area: Double = side * side
  override def name: String = "square"
}

class Lib {
  def twice(x: Int): Int = x * 2
  def twice(s: String): String = s + s
  def max[T <: Comparable[T]](a: T, b: T): T = if (a.compareTo(b) >= 0) a else b
  def sum(xs: List[Int]): Int = xs.sum
  def sizes(xs: List[_]): Int = xs.size
  def size(x: Int): Int = x
  def size(xs: List[_]): Int = xs.size
}
