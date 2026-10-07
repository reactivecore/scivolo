//> using scala 3.9.0
object Init:
  val logged = println("init")
  val unit: Unit = ()
  var slot: Unit = ()
  private val hidden = println("hidden")
  private val count = 3
  val nothing: Null = null

class Box(x: Int):
  val shown = println(s"Box $x")
  val empty: Null = null
  def get = shown

class Never:
  val boom: Nothing = throw IllegalStateException("never")

@main def unitfields(): Unit =
  println(Init.logged)
  println(Init.unit)
  Init.slot = println("set")
  println(Init.slot)
  println(Init.nothing)
  println(Box(1).get)
  println(Box(2).empty)
