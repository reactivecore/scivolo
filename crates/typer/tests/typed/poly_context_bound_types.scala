trait Show[X]:
  def show(x: X): String

object Main:
  given Show[Int] with
    def show(x: Int): String = x.toString

  type F = [X: Show] => (x: X) => String
  val f: F = [X: Show] => (x: X) => summon[Show[X]].show(x)
  val g: [X: {Show as s}] => X => String = [X: {Show as s}] => (x: X) => s.show(x)

  def main(args: Array[String]): Unit =
    println(f(1))
    println(g(2))
