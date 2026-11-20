import scala.language.implicitConversions

opaque type Position[Buffer] = Int
object Position:
  def of[B](c: Char): Position[B] = c.toInt

trait TokenParser[Token, R]

object TextParser:
  given TP: TokenParser[Char, Position[CharSequence]]()
  given FromCharToken(using T: TokenParser[Char, Position[CharSequence]]): Conversion[Char, Position[CharSequence]] =
    c => Position.of(c)

object ViewsOnOperands:
  import TextParser.{given, *}
  val p: Position[CharSequence] = 'x'

  abstract class Unit
  object NoUnit extends Unit
  object Hour extends Unit
  case class Measure(scalar: Double, unit: Unit):
    def *(newUnit: Unit): Measure = Measure(scalar, newUnit)
  implicit def double2Measure(scalar: Double): Measure = Measure(scalar, NoUnit)
  val m = 3.0 * (Hour: Unit)

  class Op[I]
  class Rich(x: Double):
    def +[I](op: Op[I]): Op[I] = op
  implicit def toRich(x: Double): Rich = new Rich(x)
  val o = 1.0 + new Op[Int]
