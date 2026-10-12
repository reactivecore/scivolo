import scala.compiletime.*
import scala.compiletime.ops.int.*

trait Show[T] { def show(t: T): String }
object Show {
  given Show[Int] = new Show[Int] { def show(t: Int) = "int " + t }
  given Show[String] = new Show[String] { def show(t: String) = "string " + t }
}

object Test {
  type Three = 3
  final val four = 4

  inline def plusOne[N <: Int]: Int = constValue[N] + 1
  inline def opt[T]: Option[T] = constValueOpt[T]
  inline def check(inline x: Int): Unit = requireConst(x)
  inline def show[T](t: T): String = summonInline[Show[T]].show(t)
  inline def code(x: Int): String = codeOf(x)
  inline def flag(inline b: Boolean): String = codeOf(b)
  inline def name[T]: String = inline constValue[T] match {
    case 1 => "one"
    case _ => "many"
  }
  inline def sum[A <: Int, B <: Int]: Int = constValue[A + B]
  inline def size[S <: String]: Int = constValue[scala.compiletime.ops.string.Length[S]]
  inline def both[A <: Boolean, B <: Boolean]: Boolean = constValue[scala.compiletime.ops.boolean.&&[A, B]]
  inline def text[N]: String = constValue[scala.compiletime.ops.any.ToString[N]]

  def main(args: Array[String]): Unit = {
    println(plusOne[4])
    println(plusOne[Three])
    println(plusOne[four.type])
    println(opt[5])
    println(opt["s"])
    check(3)
    println(show(1))
    println(show("a"))
    println(code(4))
    println(flag(true))
    println(name[1])
    println(name[2])
    println(sum[2, 5])
    println(size["four"])
    println(both[true, false])
    println(text[42])
  }
}
