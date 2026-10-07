//> using scala 3.9.0
object Lists:
  def show(): Unit =
    println(List(1, 2, 3))
    println(List("a", "b"))
    println(Seq(1.5, 2.5))
    println(List(List(1, 2), List(3)))
    println(List[Int]())
    println(List(1, 2, 3, 4, 5, 6, 7, 8, 9).sum)
    println(List(1, 2).map(_ * 10))

  val primes = List(2, 3, 5, 7)

@main def listliterals(): Unit =
  Lists.show()
  println(Lists.primes.reverse)
