//> using scala 3.9.0
enum Color:
  case Red, Green, Blue

enum Planet:
  case Mercury, Venus

@main def enums(): Unit =
  println(Color.Green)
  println(Color.Blue.ordinal)
  println(Color.values.length)
  println(Color.valueOf("Red"))
  println(Color.fromOrdinal(1))
  println(Color.Red == Color.Red)
  println(Color.Red.hashCode == "Red".hashCode)
  println(Planet.values.map(_.toString).mkString(","))
  val c: Color = Color.Blue
  c match
    case Color.Red => println("red")
    case Color.Blue => println("blue")
    case _ => println("other")
