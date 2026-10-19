//> using scala 3.9.0
// Derives clauses: companion givens calling inline and plain `derived` methods over mirrors.

import scala.deriving.Mirror
import scala.compiletime.{constValue, constValueTuple, summonInline}

trait Show[A] {
  def show(a: A): String
}

trait Fields[T] {
  def shows(t: Any): List[Any]
}

object Fields {
  given Fields[EmptyTuple] = (t: Any) => Nil
  given [H, T <: Tuple](using h: Show[H], t: Fields[T]): Fields[H *: T] = (x: Any) =>
    t.shows(x.asInstanceOf[H *: T].tail).prepended(h.show(x.asInstanceOf[H *: T].head))
}

object Show {
  given Show[Int] = (a: Int) => a.toString
  given Show[String] = (a: String) => "\"" + a + "\""

  inline def derived[A](using m: Mirror.ProductOf[A]): Show[A] =
    product(
      constValue[m.MirroredLabel],
      constValueTuple[m.MirroredElemLabels].toList,
      summonInline[Fields[m.MirroredElemTypes]]
    )

  def product[A](label: String, names: List[Any], fields: Fields[?]): Show[A] = new Show[A] {
    def show(a: A): String = {
      val elems = Tuple.fromProduct(a.asInstanceOf[Product])
      val parts = names.zip(fields.shows(elems)).map((n, s) => s"$n = $s")
      parts.mkString(label + "(", ", ", ")")
    }
  }
}

trait Ordinal[A] {
  def ordinal(a: A): Int
}

object Ordinal {
  def derived[A](using m: Mirror.SumOf[A]): Ordinal[A] = (a: A) => m.ordinal(a)
}

case class P(id: Int, name: String) derives Show, CanEqual
case class Q(p: P, n: Int) derives Show
case class Pair[A](a: A, b: A) derives Show, CanEqual

sealed trait Shape derives Ordinal
case class Circle(r: Int) extends Shape
case object Dot extends Shape

enum Color derives Ordinal, CanEqual {
  case Red, Green, Blue
}

object deriving {
  def main(args: Array[String]): Unit = {
    println(summon[Show[P]].show(P(1, "a")))
    println(summon[Show[Q]].show(Q(P(2, "b"), 3)))
    println(summon[Show[Pair[String]]].show(Pair("x", "y")))
    println(P(1, "a") == P(1, "a"))
    println(Pair(1, 2) == Pair(1, 3))
    println(summon[Ordinal[Shape]].ordinal(Dot))
    println(summon[Ordinal[Color]].ordinal(Color.Blue))
    println(Color.Red == Color.Red)
  }
}
