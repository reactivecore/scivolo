//> using scala 3.9.0

/** Comparing a value with `()` boxes the unit, and a method selected on a `null`-typed value
  * throws NullPointerException.
  */
def show(x: Any): String = if x == () then "unit" else x.toString

@main def nullrecv(): Unit =
  println(show(()))
  println(show(1))
  val u: Unit = ()
  println(show(u))
  val m = null
  try println(m.toString)
  catch case _: NullPointerException => println("npe")
