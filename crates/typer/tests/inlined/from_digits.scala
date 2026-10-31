import scala.language.experimental.genericNumberLiterals
import scala.util.FromDigits

case class Num(digits: String)

object Num:
  given FromDigits.Floating[Num] with
    inline def fromDigits(digits: String): Num = Num(digits)

case class Hex(digits: String, radix: Int)

object Hex:
  given FromDigits.WithRadix[Hex] with
    def fromDigits(digits: String, radix: Int): Hex = Hex(digits, radix)

object Literals:
  val whole: Num = 1234
  val decimal: Num = 12.5
  val floating: Num = 1.5e3
  val hex: Hex = 0x1F
  val plain: Int = 12
  def take(n: Num): Num = n
  val arg = take(7)
