package scala.probe

class Base(val size: Int, protected val name: String)

class Derived[A](size: Int, name: String, xs: List[A]) extends Base(size, name) {
  def this(x: A) = this(1, "one", List(x))
  def this(x: A, y: A) = this(2, "two", List(x, y))
  def first: A = xs.head
  def total: Int = size + name.length
}
