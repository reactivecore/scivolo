//> using scala 3.9.0
// Companions with their own mirror members, or extending a mirror themselves.

import scala.deriving.Mirror

case class F(x: Int)
object F { def fromProduct(p: Product): F = new F(-1) }
case class P(x: Int)
object P extends Mirror.Product {
  type MirroredMonoType = P
  def fromProduct(p: Product): P = new P(-2)
}
case object CS extends Mirror.Singleton
sealed trait T
case class T1() extends T
object T extends Mirror.Sum {
  type MirroredMonoType = T
  def ordinal(x: T): Int = 7
}

object usermirrors {
  def main(args: Array[String]): Unit = {
    println(summon[Mirror.ProductOf[F]].fromProduct(Tuple1(1)))
    println(summon[Mirror.ProductOf[P]].fromProduct(Tuple1(1)))
    println((CS.fromProduct(EmptyTuple), summon[Mirror.SumOf[T]].ordinal(T1())))
  }
}
