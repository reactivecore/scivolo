//> using scala 3.9.0
import language.experimental.erasedDefinitions
import language.experimental.saferExceptions

class Er26Proof extends compiletime.Erased
class Er26Fail extends Exception

class Er26Box(erased p: Er26Proof)(val n: Int):
  def twice(using Er26Proof): Int = n * 2

trait Er26Decoder[+T]:
  def apply(): T

object Er26erased:
  given Er26Proof = Er26Proof()

  def plain(erased x: Int, y: String): String = y + "!"

  def ctx(n: Int): (erased Er26Proof) ?=> String = s"ctx $n"

  def risky(x: Int): Int throws Er26Fail = if x > 0 then throw Er26Fail() else x

  val decoder: Er26Decoder[Er26Proof ?=> Int] = new Er26Decoder[Er26Proof ?=> Int]:
    def apply(): Er26Proof ?=> Int = 7

  def main(args: Array[String]): Unit =
    println(plain(1, "plain"))
    val f: (erased Int, String) => String = (erased x, s) => s.reverse
    println(f(2, "olleh"))
    val box = Er26Box(summon[Er26Proof])(21)
    println(box.twice)
    println(ctx(3))
    val caught =
      try risky(1)
      catch case _: Er26Fail => -1
    println(caught)
    println(decoder.apply())
