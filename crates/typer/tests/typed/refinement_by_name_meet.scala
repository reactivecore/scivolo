import scala.language.reflectiveCalls

object Meets {
  def both(x: { def a: Seq[Int] } & { val a: List[Int] }) = x.a
  def other(x: { val a: Iterable[Int] } & { def a: Seq[Int] }) = x.a
  def names(x: { def a: Seq[Int] } & { def a: List[Int] }) = x.a
}
