class C(s: String) {
  class Inner { override def toString = "Inner of " + s }
  object Inner {
    implicit def innerToString(i: Inner): String = s
  }
  implicit def mk: Inner = new Inner
}
trait Monoid[T] {
  extension (x: T) def combine(y: T): T
  def unit: T
}
class K
object K {
  extension (k: K) def twice: Int = 2
}

object Containers {
  implicit object Pos {
    extension (x: Int) def positive: Boolean = x > 0
  }
  given StringMonoid: Monoid[String] with {
    extension (x: String) def combine(y: String): String = x.concat(y)
    def unit: String = ""
  }
  def sum[T: Monoid](xs: List[T]): T = xs.foldLeft(implicitly[Monoid[T]].unit)(_.combine(_))
  val c = new C("OK")
  def converted: String = new c.Inner
  def found = summon[c.Inner]
  def local = {
    val d = new C("local")
    val s: String = new d.Inner
    s
  }
  def e1 = 1.positive
  def e2 = "a".combine("b")
  def e3 = sum(List("x", "y"))
  def e4 = (new K).twice
}
