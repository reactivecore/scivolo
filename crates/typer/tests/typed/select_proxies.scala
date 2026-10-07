object Outer:
  class Inner(val x: Int)
  class Gen[T](val t: T)

class Top[T](val t: T)
class Box[T](x: T) { def get: T = x }

object Main:
  val d: Top[String] = Top("s")
  def run(): Int =
    val a = Outer.Inner(3)
    val b = Outer.Gen("s")
    val e = new Box("s")
    a.x + b.t.length + d.t.length + e.get.length
