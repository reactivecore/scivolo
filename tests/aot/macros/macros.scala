import macrolib.Macros.*

@main def macros(): Unit =
  println(twice(21))
  println(greet("Bob"))
  println(listOf('a', 'b'))
  val n = 4
  println(plusOneTwice(n))
