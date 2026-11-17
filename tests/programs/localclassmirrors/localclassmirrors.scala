//> using scala 3.9.0
// A sum mirror summoned in a method of a local class of an inline body.

import scala.deriving.Mirror

sealed trait Shape
sealed trait Round extends Shape
case class Circle(r: Int) extends Round
case class Oval(a: Int, b: Int) extends Round
case class Square(s: Int) extends Shape

trait Count {
  def count(s: Shape): Int
}

object Count {
  inline def derived: Count = {
    class Local extends Count {
      def count(s: Shape): Int = s match {
        case r: Round => compiletime.summonInline[Mirror.SumOf[Round]].ordinal(r) + 1
        case _ => 0
      }
    }
    new Local
  }
}

object localclassmirrors {
  def main(args: Array[String]): Unit = {
    val c = Count.derived
    println(c.count(Circle(1)))
    println(c.count(Oval(1, 2)))
    println(c.count(Square(3)))
  }
}
