case class Box[A](value: A)

object Main {
  def a: Int = Box("ab").value.length
  def b: String = Box(1).value.toString
  def c: Int = Box(List(1, 2)).value.head
}
