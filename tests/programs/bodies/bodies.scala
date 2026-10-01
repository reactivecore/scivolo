//> using scala 3.9.0
object Init:
  println("init")
  val a = "a"
  println(a)

object Secrets:
  private val secret = "s"
  def reveal: String = secret

object Consts:
  final val k = 3
  final val s = "str"

object Counter:
  var count = 0
  var name = "n"

class Holder(x: String, val y: String):
  def both: String = x

@main def bodies(): Unit =
  println(Init.a)
  println(Secrets.reveal)
  println(Consts.k)
  println(Consts.s)
  println(Counter.count)
  println(Counter.name)
  println(new Holder("x", "y").both)
