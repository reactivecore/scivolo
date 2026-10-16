//> using scala 3.9.0
// Companions of sealed classes: sum mirrors with `ordinal`, none where a child is anonymous or
// no generic product.

sealed trait S
object S
case class A(x: Int) extends S
case object B extends S
final class C extends S

sealed abstract class T
object T {
  case class D(s: String) extends T
  case object E extends T
}

sealed trait U
object U { def make: U = new U {} }

class Box(n: Int) {
  sealed trait Item
  object Item {
    case class Small(k: Int) extends Item
    case class Big(k: Int) extends Item
  }
  import Item.*
  def size(i: Item): Int = i match {
    case Small(k) => k
    case Big(k) => k * n
  }
}

object sealedsums {
  def ordinalOf(t: T): Int = {
    val m: deriving.Mirror.Sum { type MirroredMonoType = T } = T.asInstanceOf
    m.ordinal(t)
  }

  def main(args: Array[String]): Unit = {
    println(ordinalOf(T.D("x")))
    val b = new Box(3)
    println(b.size(b.Item.Small(2)) + b.size(b.Item.Big(2)))
    println(U.make.isInstanceOf[U])
  }
}
