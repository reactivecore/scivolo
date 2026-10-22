import scala.compiletime.summonInline

trait A
class B extends A

given ok: A = B()

object Test:
  transparent inline given loop: A = summonInline[A]
  val a: A = summon[A]
