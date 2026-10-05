//> using scala 3.9.0
class Cell[T](val value: T):
  def get: T = value

object G:
  def id[T](x: T): T = x
  def first[A, B](a: A, b: B): A = a
  def pick[T](c: Boolean, a: T, b: T): T = if c then a else b
  def bounded[T <: CharSequence](x: T): Int = x.length()
  def ints(): Int =
    val n: Int = id(3)
    n + 1
  def strings(): String = id[String]("a") + first("x", 2)
  def cells(): Int =
    val c = new Cell[Int](5)
    c.get + 1

@main def generics(): Unit =
  println(G.ints())
  println(G.strings())
  println(G.pick(false, "a", "b"))
  println(G.bounded("abc"))
  println(G.cells())
  println(new Cell[String]("s").get)
