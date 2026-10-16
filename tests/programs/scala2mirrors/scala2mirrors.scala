//> using scala 3.9.0
// Mirrors of Scala 2 library classes: an anonymous product mirror, a singleton proxy.

import scala.deriving.Mirror

object scala2mirrors {
  def main(args: Array[String]): Unit = {
    val msome = summon[Mirror.ProductOf[Some[Int]]]
    println(msome.fromProduct(Tuple1(5)))
    val mnone = summon[Mirror.Of[None.type]]
    println(mnone.fromProduct(EmptyTuple))
  }
}
