import scala.compiletime.ops.int.{^ as ^^, *}
import scala.compiletime.ops.long.{^ as ^^^, BitwiseAnd as AndL, << as <<<, >>> as >>>>}

object Test {
  val a: BitwiseOr[100 << 2 >>> 2 >> 2 ^^ 3, BitwiseAnd[7, 7]] = 31
  val b: 1 << 33 = 2
  val c: -8 >>> 28 = 15
  val d: -8 >> 1 = -4
  val e: NumberOfLeadingZeros[1] = 31
  val f: AndL[6L, 3L] = 2L
  val g: 1L <<< 33L = 8589934592L
  val h: -8L >>>> 60L = 15L
  val i: 5L ^^^ 3L = 6L
  final val one = 1
  val j: 123 & Int = 123
  val k: j.type + one.type = 124
  val l: S[j.type] = 124
}
