//> using options -source:3.5
class Base
class Sub extends Base
trait Show[-T] { def show(t: T): String }

object Ranked {
  given Base = Base()
  given Sub = Sub()
  val b = summon[Base]

  implicit val showAny: Show[Any] = _ => "any"
  implicit val showInt: Show[Int] = _ => "int"
  val s = implicitly[Show[Int]]
}
