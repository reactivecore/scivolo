case class Box[A](value: A)

object Main {
  def a: Int = Box("ab").value.length
  def b: String = Box(1).value.toString
  def c: Int = Box(List(1, 2)).value.head
  def avg(es: List[Double]): Double = es.sum / es.size
  def sum(es: List[Double]): String = es.sum.toString
}
