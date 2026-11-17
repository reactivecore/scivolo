class TupleKeptHolder { type L[X] = Option[X] }
class tupleKeptAnn[T] extends scala.annotation.StaticAnnotation

class TupleKeptArgs(val q: String) {
  val v: String = "a"
  def singleton(p: (Some[v.type], Some[q.type])) = p match {
    case t @ (a: Option[v.type], b: Option[q.type]) => t
  }
  def literalAndThis(p: (Some[1], Some[this.type])) = p match {
    case t @ (a: Option[1], b: Option[TupleKeptArgs.this.type]) => t
  }
  def pathTycon(p: (Some[String], Int), h: TupleKeptHolder) = {
    import h.L
    p match {
      case t @ (o: L[String], y) => t
    }
  }
  def infixAndParens(p: (Right[Int, String], Some[String])) = p match {
    case t @ (a: (Int Either String), b: (Option[String])) => t
  }
  def rewrittenArgs(p: (Some[String], Some[String])) = p match {
    case t @ (a: Option[(String)], b: Option[String | Int]) => t
  }
  def annotations(p: (Some[String], Some[String], Some[String])) = p match {
    case t @ (a: Option[String @tupleKeptAnn], b: Option[String @deprecated], c: Option[String @deprecated("a", "b")]) =>
      t
  }
}
