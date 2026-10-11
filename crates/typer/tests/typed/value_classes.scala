class Outer {
  class Member(val a: Int) extends AnyVal
}
class Fields(val a: Int) extends AnyVal {
  val y = 2
  var z = 3
  println("hi")
  class Inner
}
class Two(a: Int, b: Int) extends AnyVal
class None() extends AnyVal
class Wrap(val f: Fields) extends AnyVal
trait T extends AnyVal
object O extends AnyVal
abstract class Abs(val a: Int) extends AnyVal
class Var(var a: Int) extends AnyVal
class ByName(a: => Int) extends AnyVal
type Alias = AnyVal
class Aliased(val a: Int) extends Alias
object Test {
  class Ok(val a: Int) extends AnyVal
  def f = {
    class Local(val a: Int) extends AnyVal
    1
  }
}
