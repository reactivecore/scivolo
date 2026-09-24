import Macros.*

@main def useMacro(): Unit =
  val x = 1.5
  println(s"power: ${power(x, 5)}")
  println(s"debug: ${debug(x * 2 + 1)}")
