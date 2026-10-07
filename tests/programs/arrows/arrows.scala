//> using scala 3.9.0
def pairs(n: Int): List[(Int, String)] =
  if n == 0 then Nil else pairs(n - 1).prepended(n -> n.toString)

@main def arrows(): Unit =
  println(1 -> "one")
  println("pi" -> 3.14)
  val k = "key"
  println(k -> k.length)
  println((1 -> 2) -> 3)
  println(pairs(3))
  println(true -> 'c')
