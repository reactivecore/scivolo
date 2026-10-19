trait Show[A]
object Show { def derived[A]: Show[A] = ??? }
object Codec { trait AsEncoder[A]; trait AsDecoder[A] }
class Box(val count: Int) { def total: Int = count; def totals: Int = count }
class Base { def abd: Int = 1; def abe: Int = 1 }
class Sub extends Base { override def abe: Int = 2; def abf: Int = 3; def abg: Int = 4 }
case class Person(id: Int) derives Codec.AsEncodr
case class Pet(id: Int) derives Codec.Missing

object MemberHints {
  val b = new Box(1)
  val a = b.cont
  val c = b.totl
  val e: Codec.AsEncodr[Int] = ???
  val f: Codec.AsCoder[Int] = ???
  val g = b.qwerty
  val s = new Sub
  val h = s.abc
}
