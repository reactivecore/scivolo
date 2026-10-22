trait Show[X]:
  def show(x: X): String

object Main:
  given Show[Int] with
    def show(x: Int) = x.toString
  val direct = [X: Show] => (x: X) => summon[Show[X]].show(x)
  val named = [X: {Show as s}] => (x: X) => s.show(x) + "!"
  val curried = [X: Show] => (x: X) => (y: X) => summon[Show[X]].show(y)
  def main(args: Array[String]): Unit =
    println(direct(42))
    println(named(1))
    println(curried(1)(2))
