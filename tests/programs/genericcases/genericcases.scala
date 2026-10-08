//> using scala 3.9.0
case class Box[A](value: A)
case class Pair[A, B](a: A, b: B):
  def swap: Pair[B, A] = Pair(b, a)

@main def genericcases(): Unit =
  val b = Box(1)
  println(b)
  println(b.value + 1)
  println(b.copy(value = "s"))
  val p = Pair("x", 2.5)
  println(p.swap)
  p match
    case Pair(s, d) => println(s"$s $d")
  println(Box("ab").value)
