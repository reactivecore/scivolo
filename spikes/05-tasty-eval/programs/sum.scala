//> using scala 3.9.0

object Sum:
  def main(args: Array[String]): Unit =
    var i = 1
    var acc = 0L
    while i <= 100000000 do
      acc += i
      i += 1
    println("sum = " + acc)
