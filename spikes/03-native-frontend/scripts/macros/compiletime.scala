//> using scala 3.9.0
import scala.compiletime.{constValue, erasedValue, summonInline, error, codeOf}
import scala.compiletime.ops.int.*
import scala.deriving.Mirror

inline def sizeOf[T]: Int = inline erasedValue[T] match
  case _: Byte   => 1
  case _: Int    => 4
  case _: Long   => 8
  case _: String => error("no fixed size")

inline def labels[T](using m: Mirror.ProductOf[T]): List[String] =
  names[m.MirroredElemLabels]

inline def names[L <: Tuple]: List[String] = inline erasedValue[L] match
  case _: EmptyTuple => Nil
  case _: (h *: t)   => constValue[h].toString :: names[t]

trait Show[A]:
  def show(a: A): String

object Show:
  given Show[Int] = _.toString
  given Show[String] = s => s"\"$s\""
  inline def derived[A](using m: Mirror.ProductOf[A]): Show[A] =
    val shows = summonAll[m.MirroredElemTypes]
    val ls = labels[A]
    (a: A) =>
      val vs = a.asInstanceOf[Product].productIterator.toList
      ls.zip(vs).zip(shows).map { case ((l, v), s) => s"$l=${s.asInstanceOf[Show[Any]].show(v)}" }
        .mkString("(", ", ", ")")

inline def summonAll[T <: Tuple]: List[Show[?]] = inline erasedValue[T] match
  case _: EmptyTuple => Nil
  case _: (h *: t)   => summonInline[Show[h]] :: summonAll[t]

case class Person(name: String, age: Int) derives Show

@main def compiletimeDemo(): Unit =
  val three: 1 + 2 = 3
  println(s"sizes: ${sizeOf[Byte]} ${sizeOf[Int]} ${sizeOf[Long]} const=${constValue[3 * 4]} $three")
  println(s"labels: ${labels[Person]}")
  println(s"show: ${summon[Show[Person]].show(Person("Ada", 36))}")
  println(s"codeOf: ${codeOf(1 + 2 * 3)}")
