//> using scala 3.9.0

final class Implicit()

final class Id[+A, -U](val value: A):
  def map[B](f: A => B)(using Implicit): Id[B, U] = throw new IllegalStateException("map")
  def flatMap[B, V <: U](f: A => Id[B, V]): Id[B, V] = f(value)

def pure(a: Int): Id[Int, Any] = Id(a)

def viewed[B](x: Int)(implicit ev: Int => B): B = ev(x)

@main def evidence =
  given Implicit = Implicit()
  println(List(List(1, 2), List(3)).flatten)
  println(Seq((1, "a"), (2, "b")).unzip)
  println(Map(1 -> 1).toMap)
  println(List(1 -> "x", 2 -> "y").toMap)
  println(viewed(3))
  println((1L to 10L).sum)
  val id = for a <- pure(4); b <- pure(a * 10) yield b
  println(id.value)
  println(for x <- List(1, 2, 3) yield x)
  println(for x <- Option(5) yield ())
