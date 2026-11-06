enum Color extends java.lang.Enum[Color] {
  case Red, Green, Blue
}

enum Planet(val mass: Double) extends java.lang.Enum[Planet] {
  case Mercury extends Planet(3.3e23)
  case Earth extends Planet(5.97e24)
}

object Javaenums {
  def main(args: Array[String]): Unit = {
    println(Color.Red)
    println(Color.Green.ordinal)
    println(Color.values.toList)
    println(Color.valueOf("Blue"))
    println(Color.Blue.compareTo(Color.Red))
    println(Planet.Earth.mass)
    println(Planet.values.map(_.name).mkString(","))
  }
}
