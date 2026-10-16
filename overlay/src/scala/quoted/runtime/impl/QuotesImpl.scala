package scala.quoted.runtime.impl

import scala.quoted.{Expr, FromExpr, Quotes, Type}
import scala.quoted.runtime.{QuoteMatching, QuoteUnpickler}

/** The `Quotes` a macro runs with: quotes unpickled into the frontend's trees, everything else
 *  `Unsupported`.
 */
final class QuotesImpl extends Quotes, QuoteUnpickler, QuoteMatching {
  /** Its getter is a VM native throwing `Unsupported`. */
  val reflect: reflectModule = null

  extension [T](self: Expr[T]) {
    def show: String = throw new Unsupported("Expr.show")
    def matches(that: Expr[Any]): Boolean = throw new Unsupported("Expr.matches")
    def valueOrAbort(using fromExpr: FromExpr[T]): T = throw new Unsupported("Expr.valueOrAbort")
  }

  extension (self: Expr[Any]) {
    def isExprOf[X](using Type[X]): Boolean = throw new Unsupported("Expr.isExprOf")
    def asExprOf[X](using Type[X]): Expr[X] = throw new Unsupported("Expr.asExprOf")
  }

  def unpickleExpr[T](
      pickled: String | List[String],
      typeHole: (Int, Seq[Any]) => Type[?],
      termHole: (Int, Seq[Any], Quotes) => Expr[?]
  ): Expr[T] = throw new Unsupported("a quote of Scala 3.0 or 3.1")

  def unpickleExprV2[T](
      pickled: String | List[String],
      types: Seq[Type[?]] | Null,
      termHole: Null | ((Int, Seq[Type[?] | Expr[Any]], Quotes) => Expr[?])
  ): Expr[T] = {
    val quote = QuoteHost.unpickleExpr(text(pickled), typeIds(types))
    val holes = QuoteHost.holeCount(quote)
    var hole = 0
    while (hole < holes) {
      if (termHole == null) throw new Unsupported("a quote with holes but no hole function")
      val args = (0 until QuoteHost.holeArgCount(quote, hole)).map { i =>
        val arg = QuoteHost.holeArg(quote, hole, i)
        if (arg >= 0) QuoteHost.expr(arg) else QuoteHost.tpe(-1 - arg)
      }
      val filled = termHole.nn(QuoteHost.holeIndex(quote, hole), args, this)
      QuoteHost.fillHole(quote, hole, QuoteHost.exprId(filled))
      hole += 1
    }
    QuoteHost.expr(quote).asInstanceOf[Expr[T]]
  }

  def unpickleType[T <: AnyKind](
      pickled: String | List[String],
      typeHole: (Int, Seq[Any]) => Type[?],
      termHole: (Int, Seq[Any], Quotes) => Expr[?]
  ): Type[T] = throw new Unsupported("a quoted type of Scala 3.0 or 3.1")

  def unpickleTypeV2[T <: AnyKind](pickled: String | List[String], types: Seq[Type[?]] | Null): Type[T] =
    QuoteHost.tpe(QuoteHost.unpickleType(text(pickled), typeIds(types))).asInstanceOf[Type[T]]

  object ExprMatch extends ExprMatchModule {
    def unapply[TypeBindings, Tup <: Tuple](scrutinee: Expr[Any])(using pattern: Expr[Any]): Option[Tup] =
      throw new Unsupported("a quote pattern")
  }

  object TypeMatch extends TypeMatchModule {
    def unapply[TypeBindings, Tup <: Tuple](scrutinee: Type[?])(using pattern: Type[?]): Option[Tup] =
      throw new Unsupported("a quoted type pattern")
  }

  private def text(pickled: String | List[String]): String = pickled match {
    case s: String       => s
    case l: List[String] => l.mkString
  }

  private def typeIds(types: Seq[Type[?]] | Null): Array[Int] =
    if (types == null) new Array[Int](0) else types.nn.map(QuoteHost.typeId).toArray
}
