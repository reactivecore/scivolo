//> using scala 3.9.0
class Box(val v: String):
  def get: String = v

trait Named:
  def name: String = "named"

class Thing extends Named

@main def classes(): Unit =
  println(new Box("box").get)
  println(new Box("v").v)
  println(new Thing().name)
