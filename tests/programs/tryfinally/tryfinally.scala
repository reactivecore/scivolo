//> using scala 3.9.0
object Cleanup:
  var log = ""

  def plain(n: Int): Int =
    try n * 2
    finally log += "p"

  def parse(s: String): Int =
    try s.toInt
    catch
      case _: NumberFormatException => -1
      case e: IllegalArgumentException => -2
    finally log += s"[$s]"

  def thrown(): Unit =
    try throw IllegalStateException("boom")
    finally log += "t"

  def early(n: Int): Int =
    try
      if n > 0 then return n
      0
    finally log += "e"

  def loop(n: Int): Int =
    var i = 0
    var sum = 0
    try
      while i < n do
        sum += i
        i += 1
      sum
    finally log += "l"

  def nested(): String =
    try
      try "inner"
      finally log += "i"
    finally log += "o"

@main def tryfinally(): Unit =
  println(Cleanup.plain(2))
  println(Cleanup.parse("7"))
  println(Cleanup.parse("x"))
  try Cleanup.thrown()
  catch case e: IllegalStateException => println(e.getMessage)
  println(Cleanup.early(3))
  println(Cleanup.early(-1))
  println(Cleanup.loop(4))
  println(Cleanup.nested())
  println(Cleanup.log)
