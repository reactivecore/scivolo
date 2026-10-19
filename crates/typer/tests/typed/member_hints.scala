trait Show[A]
object Show { def derived[A]: Show[A] = ??? }
object Codec { trait AsEncoder[A]; trait AsDecoder[A] }
class Box(val count: Int) { def total: Int = count; def totals: Int = count }
case class Person(id: Int) derives Codec.AsEncodr
case class Pet(id: Int) derives Codec.Missing

object MemberHints {
  val b = new Box(1)
  val a = b.cont
  val c = b.totl
  val e: Codec.AsEncodr[Int] = ???
  val f: Codec.AsCoder[Int] = ???
  val g = b.qwerty
}
