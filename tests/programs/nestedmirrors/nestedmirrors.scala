//> using scala 3.9.0
// Product mirrors of a case class and of the case classes nested in its companion.

import scala.deriving.Mirror

case class Name(value: String)
object Name {
  case class Machine(value: String)
  object Inner {
    case class Deep(value: String, n: Int)
  }
}

object nestedmirrors {
  def main(args: Array[String]): Unit = {
    val m = summon[Mirror.ProductOf[Name]]
    val n = summon[Mirror.ProductOf[Name.Machine]]
    val d = summon[Mirror.ProductOf[Name.Inner.Deep]]
    println(m.fromProduct(Tuple1("a")))
    println(n.fromProduct(Tuple1("b")))
    println(d.fromProduct(("c", 1)))
  }
}
