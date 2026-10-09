//> using scala 3.9.0
class Planets extends Enumeration:
  val Mercury = Value
  val Venus = Value(3)

object Enums:
  object Color extends Enumeration:
    val Red, Green = Value
    val Blue = Value(5)
    val Gray = Value(null)
    val Pink = Value(7, null)
    val Named = Value("named")

  object Day extends Enumeration(10):
    type Day = Value
    val Mon: Day =
      Value
    private val Tue = Value(
      20
    )
    def tue = Tue

import Enums.*

@main def enumerations(): Unit =
  println(Color.Red)
  println(Color.Green)
  println(Color.Blue.id)
  println(Color.Gray)
  println(Color.Pink)
  println(Color.Named)
  println(Color.values.toList)
  println(Color.withName("Pink").id)
  println(Day.Mon.id)
  println(Day.tue)
  val p = Planets()
  println(p.Mercury)
  println(p.Venus.id)
