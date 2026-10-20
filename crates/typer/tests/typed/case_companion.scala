case class Plus(a: Int, b: Int)

trait Idioms {
  case class Pair(a: Int, b: Int)
  def pair: Pair = Test.curry2(Pair)(3)(4)
}

object Test extends Idioms {
  def curry2[A, B, C](f: (A, B) => C): A => B => C = a => b => f(a, b)
  def main(args: Array[String]): Unit = {
    println(curry2(Plus)(1)(2))
    println(pair)
  }
}
