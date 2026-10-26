import scala.compiletime.{erasedValue, summonInline}

trait Show[T]:
  def show: String

given Show[Int] with
  def show = "Int"

given Show[String] with
  def show = "String"

class Gen[T]:
  type Shape = (T, List[T], String)

inline def shows[E <: Tuple]: List[String] =
  inline erasedValue[E] match
    case _: (List[e] *: es) => ("List of " + summonInline[Show[e]].show) :: shows[es]
    case _: (e *: es) => summonInline[Show[e]].show :: shows[es]
    case _: EmptyTuple => Nil

inline def describe[T](g: Gen[T]): List[String] = shows[g.Shape]

@main def Lw26shape(): Unit =
  val g = new Gen[Int]
  println(shows[g.Shape])
  println(describe(new Gen[String]))
