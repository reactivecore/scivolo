//> using scala 3.9.0
trait Shape:
  def area(): Int

trait Square extends Shape:
  def area(): Int

object apps extends App:
  println("start")
  val answer = 21 * 2
  println(answer)
  println(executionStart > 0)
  val ints: Array[Int] = Array(1, 2, 3)
  println(ints.toArray.length)
  val square: Square = () => 16
  println(square.area())
