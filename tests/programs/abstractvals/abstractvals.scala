//> using scala 3.9.0
trait Shape:
  val name: String
  def area: Double
  def describe: String = s"$name with area $area"

class Square(side: Double) extends Shape:
  val name = "square"
  def area: Double = side * side

case class Circle(name: String, r: Double) extends Shape:
  def area: Double = 3.0 * r * r

abstract class Animal:
  val sound: String
  def speak: String = sound + "!"

class Dog extends Animal:
  val sound = "woof"

object Cat extends Animal:
  val sound = "meow"

@main def abstractvals(): Unit =
  println(Square(2).describe)
  println(Circle("circle", 1).describe)
  println(Dog().speak)
  println(Cat.speak)
