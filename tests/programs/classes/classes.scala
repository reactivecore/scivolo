//> using scala 3.9.0
class Box(val v: String):
  def get: String = v

trait Named:
  def name: String = "named"

class Thing extends Named

class Renamed(override val name: String) extends Named

class Fixed extends Named:
  override val name: String = "fixed"

@main def classes(): Unit =
  println(new Box("box").get)
  println(new Box("v").v)
  println(new Thing().name)
  println(new Renamed("renamed").name)
  println(new Fixed().name)
