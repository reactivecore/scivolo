enum Color extends java.lang.Enum[Color]:
  case Red, Green
  case Blue extends Color

enum Vehicle(val wheels: Int) extends java.lang.Enum[Vehicle]:
  case Bike extends Vehicle(2)
  case Car extends Vehicle(4)

object Ty24JavaEnums:
  def main(args: Array[String]): Unit =
    println(Color.Red)
    println(Color.Blue.name + " " + Color.Blue.ordinal)
    println(Color.values.map(_.ordinal).mkString(","))
    println(Vehicle.Car.toString + " " + Vehicle.Car.wheels + " " + Vehicle.Car.ordinal)
    println(Color.Red.compareTo(Color.Green))
    println(Color.valueOf("Green").productPrefix)
