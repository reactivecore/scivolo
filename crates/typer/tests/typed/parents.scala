class Foo[+A] { def count: Int = 1 }
class Bar[+A] extends Foo[A]
class Base protected ()
class Derived extends Base
case class Empty()
class A(val x: Any)

object Main:
  val one: 1 = 1
  val v = new A(this) {}
  def run(): Int =
    val b = new Bar[String]
    Empty() match
      case Empty() => b.count + one
