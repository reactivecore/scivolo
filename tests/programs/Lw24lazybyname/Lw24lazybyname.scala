//> using scala 3.9.0
object Lw24lazybyname:
  def twice(n: => Int): Int = n + n

  lazy val total: Int =
    lazy val base = args0.length + 20
    twice(base)

  val args0 = Array(1, 2)

  def main(args: Array[String]): Unit = println(total)
