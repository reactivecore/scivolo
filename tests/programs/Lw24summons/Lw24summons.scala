//> using scala 3.9.0
import scala.compiletime.{summonFrom, summonInline}

trait Lw24Show[T]:
  def show(x: T): String

object Lw24Show:
  given Lw24Show[Int] with
    def show(x: Int) = s"int $x"
  given Lw24Show[String] with
    def show(x: String) = s"string $x"

  inline def show[T](x: T): String = summonInline[Lw24Show[T]].show(x)

  transparent inline def twice[T](x: T): String = show(x) + " / " + show(x)

  inline def viaCase[T](x: T): String = summonFrom {
    case s: Lw24Show[T] => "case " + show(x)
    case _ => "none"
  }

  inline def viaUsed[T](x: T): String = summonFrom {
    case s: Lw24Show[T] => s.show(x) + " and " + show(x)
  }

object Lw24Foo:
  inline def unapply(s: String): Boolean = Lw24Show.show(s) == "string foo"

object Lw24summons:
  inline given lw24Long: Lw24Show[Long] = new Lw24Show[Long]:
    def show(x: Long) = s"long ${summonInline[Lw24Show[Int]].show(x.toInt)}"

  def main(args: Array[String]): Unit =
    assert(Lw24Show.show(3) == "int 3")
    println(Lw24Show.twice(4))
    println(Lw24Show.viaCase("x"))
    println(Lw24Show.viaCase(1.5))
    println(Lw24Show.viaUsed(6))
    println(summon[Lw24Show[Long]].show(5L))
    val foo: String = "foo"
    foo match
      case Lw24Foo() => println("foo matched")
      case _ => println("no match")
