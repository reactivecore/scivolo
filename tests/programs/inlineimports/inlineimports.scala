import scala.compiletime.{summonFrom, summonInline}
import scala.compiletime.testing.typeChecks

trait Show[T] { def show(t: T): Any }
class IntShow extends Show[Int] { override def show(t: Int): String = "cov " + t }
object Inst { given Show[Int] { def show(t: Int) = "inst " + t } }
object Other { given Show[Int] { def show(t: Int) = "other " + t } }
object Cov { given IntShow = new IntShow }
object Wild { implicit val w: Show[Int] = new Show[Int] { def show(t: Int) = "wild " + t } }
object GivenOnly { given g: Show[Int] { def show(t: Int) = "given only " + t } }
object Names { val s: Int = 5 }

inline def viaGiven(x: Int): Any = { import Inst.given; summonInline[Show[Int]].show(x) }

inline def viaWildcard(x: Int): Any = { import Wild.*; summonInline[Show[Int]].show(x) }

inline def viaCov(x: Int): Any = { import Cov.given; summonInline[Show[Int]].show(x) }

inline def nested(x: Int): String = {
  val a = { import Inst.given; summonInline[Show[Int]].show(x) }
  val b = summonFrom { case s: Show[Int] => s.show(x); case _ => "none" }
  s"$a/$b"
}

inline def notByWildcard(x: Int): Any =
  import GivenOnly.*
  summonFrom { case s: Show[Int] => s.show(x); case _ => "no given" }

inline def shadowing(x: Int): Any = { import Names.*; summonInline[Show[Int]].show(x + s) }

inline def checks: (Boolean, Boolean) = {
  import Inst.given
  val inner = { import Names.s; typeChecks("s + 1") }
  (typeChecks("summon[Show[Int]]"), inner)
}

object inlineimports {
  def main(args: Array[String]): Unit = {
    println(viaGiven(1))
    println(viaWildcard(2))
    println(viaCov(3))
    println(nested(4))
    println(notByWildcard(5))
    println(checks)
    locally {
      import Other.given
      println(viaGiven(6))
      println(nested(7))
    }
    locally {
      given s: Show[Int] { def show(t: Int) = "local " + t }
      println(shadowing(8))
    }
  }
}
