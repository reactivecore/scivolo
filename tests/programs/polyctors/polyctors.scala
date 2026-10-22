//> using scala 3.9.0
class Mapper[A](f: A => A) {
  def this() = this(a => a)
  def run(x: A): A = f(x)
}

class Boxed[A] private (val size: Int) {
  def this() = this(0)
  def this(xs: List[A]) = this(xs.length)
}

object polyctors {
  def main(args: Array[String]): Unit = {
    println(new Mapper[Int]().run(3))
    println(new Mapper[String](_ + "!").run("hi"))
    println(new Boxed[String]().size)
    println(new Boxed(List(1, 2, 3)).size)
  }
}
