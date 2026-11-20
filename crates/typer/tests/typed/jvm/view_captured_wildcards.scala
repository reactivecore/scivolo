import scala.language.implicitConversions

class ViewCapturedWildcards {
  abstract class Liftable[T]
  implicit def classIsLiftable[T]: Liftable[Class[T]] = new Liftable[Class[T]] {}

  class Expr[+T]
  implicit class LiftExprOps[T](val x: T) {
    def toExpr(implicit ev: Liftable[T]): Expr[T] = ???
  }

  def runtimeClass: Class[?] = ???
  val explicit = runtimeClass.toExpr(classIsLiftable)
  val implicitly = runtimeClass.toExpr

  class Must[T](tm: () => T) {
    def must_==(other: => Any) = tm() == other
  }
  implicit def byName[T](t: => T): Must[T] = new Must(() => t)
  val uncaptured = "a".getClass must_== classOf[String]
}
