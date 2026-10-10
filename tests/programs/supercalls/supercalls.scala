//> using scala 3.9.0
abstract class Box[T](val v: T):
  def show: String = "Box(" + v + ")"

class IntBox(n: Int) extends Box[Int](n)

class Pair[A](val a: A)(val b: A):
  def both: String = a.toString + b.toString

class Twice(s: String) extends Pair[String](s)(s + s)

trait Named[T]:
  def name(t: T): String = t.toString

abstract class Base[T] extends Named[T]

object Plain extends Base[String]

@main def supercalls(): Unit =
  println(new IntBox(3).show)
  println(new Twice("x").both)
  println(Plain.name("p"))
