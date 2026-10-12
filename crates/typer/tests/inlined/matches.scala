import scala.compiletime.erasedValue
import scala.language.experimental.subCases

case class Pair(a: Int, b: String)
enum Color { case Red, Green }
sealed trait Shape
case class Circle(r: Int) extends Shape
case object Dot extends Shape

object Test {
  inline def sign(inline n: Int): String = inline if n > 0 then "pos" else "nonpos"

  inline def nested(inline a: Int, inline b: Int): String =
    inline if a > b then "gt" else inline if a < b then "lt" else "eq"

  inline def kind(x: Any): String = inline x match {
    case _: Int => "int"
    case s: String => s
    case _ => "other"
  }

  inline def lit(inline n: Int): Int = inline n match {
    case 0 => 10
    case 1 | 2 => 20
    case k if k > 5 => k * 2
    case _ => 0
  }

  inline def fact(inline n: Int): Int = inline n match {
    case 0 => 1
    case _ => n * fact(n - 1)
  }

  inline def first(p: Pair): Int = inline p match {
    case Pair(a, _) => a + 1
  }

  inline def opt(o: Option[Int]): Int = inline o match {
    case Some(x) => x
    case None => -1
  }

  inline def col(c: Color): String = inline c match {
    case Color.Red => "red"
    case Color.Green => "green"
  }

  inline def shape(s: Shape): Int = inline s match {
    case Circle(r) => -r
    case Dot => 0
  }

  inline def sub(o: Option[Int]): String = inline o match {
    case Some(x) if x match
      case 1 => "one"
      case 2 => "two"
    case _ => "other"
  }

  inline def blocky(inline b: Boolean): Int = inline { val u = 1; b } match {
    case true => 1
    case false => 0
  }

  inline def name[T]: String = inline erasedValue[T] match {
    case _: Int => "int"
    case _: String => "string"
    case _ => "other"
  }

  inline def isUnit[T]: Boolean = inline erasedValue[T] match {
    case _: Unit => true
    case _ => false
  }

  inline def both[A, B]: String = inline if isUnit[A] && isUnit[B] then "units" else name[A] + name[B]

  def main(args: Array[String]): Unit = {
    println(sign(3))
    println(nested(1, 2))
    println(nested(2, 2))
    println(kind(1))
    println(kind("s"))
    println(lit(0))
    println(lit(2))
    println(lit(7))
    println(lit(4))
    println(fact(4))
    println(first(Pair(1, "x")))
    println(opt(Some(3)))
    println(opt(None))
    println(col(Color.Green))
    println(shape(Circle(3)))
    println(shape(Dot))
    println(sub(Some(2)))
    println(sub(None))
    println(blocky(true))
    println(name[Int])
    println(name[Boolean])
    println(both[Unit, Unit])
    println(both[Int, Unit])
  }
}
