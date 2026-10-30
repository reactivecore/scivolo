class Outer {
  class Inner { def sum = 1 }
  def boxed: Box[Inner] = Box(new Inner)
}
class Box[T](x: T) { def get: T = x }

object wildcardmembers {
  def f(): Outer = new Outer

  def main(args: Array[String]): Unit = {
    val o = f()
    val c: Box[? <: Outer#Inner] = Box(new o.Inner)
    println(c.get.sum)
    val b = f().boxed
    println(b.get.sum)
  }
}
