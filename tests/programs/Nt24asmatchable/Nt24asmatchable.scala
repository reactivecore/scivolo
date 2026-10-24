//> using scala 3.9.0
import scala.compiletime.asMatchable

object Nt24asmatchable {
  def show[X](x: X): String = x.asMatchable match {
    case i: Int => s"int $i"
    case s: String => s"string $s"
    case _ => "other"
  }

  def main(args: Array[String]): Unit = {
    println(show(1))
    println(show("a"))
    println(show(2.0))
  }
}
