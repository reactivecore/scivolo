class Inv[T](val t: T)
trait Show[T]
trait Low { implicit val intShow: Show[Inv[Int]] = new Show[Inv[Int]] {} }
object NestedParamVars extends Low {
  implicit val anyShow: Show[Inv[Any]] = new Show[Inv[Any]] {}
  def sh[T](t: T)(implicit s: Show[T]): Show[T] = s
  val s = sh(new Inv(1))
}
