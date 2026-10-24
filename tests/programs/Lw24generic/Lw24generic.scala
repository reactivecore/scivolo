//> using scala 3.9.0
import scala.compiletime.{constValue, erasedValue, summonInline}

trait Lw24Name[T]:
  def name: String

object Lw24Name:
  given Lw24Name[Int] with
    def name = "Int"
  given Lw24Name[String] with
    def name = "String"

class Lw24Tagged[P <: String & Singleton, T]:
  inline def describe: String = inline erasedValue[P] match
    case _: "short" => "short " + summonInline[Lw24Name[T]].name
    case _ => constValue[P] + " " + summonInline[Lw24Name[T]].name

object Lw24generic:
  def main(args: Array[String]): Unit =
    println(Lw24Tagged["short", Int]().describe)
    println(Lw24Tagged["long", String]().describe)
