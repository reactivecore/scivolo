//> using scala 3.9.0
// Anonymous product mirrors as an inline method's argument, one made while inlining; local case
// classes of one name in several blocks.

import scala.compiletime.summonInline
import scala.deriving.Mirror

object mirrorproxies {
  inline def make[T](m: Mirror.ProductOf[T], p: Product): T = m.fromProduct(p)
  inline def made[T](p: Product): T = make(summonInline[Mirror.ProductOf[T]], p)

  def main(args: Array[String]): Unit = {
    println(make(summon[Mirror.ProductOf[Some[Int]]], Tuple1(5)))
    println(made[Some[Int]](Tuple1(6)))
    val value = 1
    println(List(1).map(x => x + value))
    { class Foo; println(new Foo != null) }
    { case class Foo(); println(summon[Mirror.ProductOf[Foo]].fromProduct(EmptyTuple)) }
  }
}
