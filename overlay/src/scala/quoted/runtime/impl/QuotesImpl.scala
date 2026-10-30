// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.quoted.{Expr, FromExpr, Quotes, Type}
import scala.quoted.runtime.{QuoteMatching, QuoteUnpickler}

/** The `Quotes` a macro runs with: quotes unpickled into the frontend's trees, quote patterns of
 *  a typed term hole, the subset of `reflect` of `ReflectImpl`; everything else `Unsupported`.
 */
final class QuotesImpl extends Quotes, QuoteUnpickler, QuoteMatching, ReflectSupport, ReflectTreeSupport, ReflectTermSupport,
    ReflectTypeSupport, ReflectTypeShapeSupport,ReflectConstantSupport, ReflectSymbolSupport, ReflectFlagSupport,
    ReflectDefinitionSupport, ReflectDeclarationSupport, ReflectPositionSupport, ReflectControlSupport,
    ReflectTypeTreeSupport, ReflectStatementSupport, ReflectPatternSupport, ReflectTypeKindSupport,
    ReflectLambdaTypeSupport, ReflectTypeBoundSupport {
  val reflect: reflectModule =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectSupport$ReflectImpl").asInstanceOf[reflectModule]

  extension [T](self: Expr[T]) {
    def show: String = ReflectOps.text("showTree", QuoteHost.exprId(self))
    def matches(that: Expr[Any]): Boolean = QuoteHost.matchPattern(QuoteHost.exprId(self), QuoteHost.exprId(that)) >= 0
    def valueOrAbort(using fromExpr: FromExpr[T]): T =
      fromExpr.unapply(self)(using this).getOrElse(throw new Unsupported("Expr.valueOrAbort of no known value"))
  }

  extension (self: Expr[Any]) {
    def isExprOf[X](using x: Type[X]): Boolean =
      ReflectOps.one("isExprOf", QuoteHost.exprId(self), QuoteHost.typeId(x)) != 0
    def asExprOf[X](using Type[X]): Expr[X] =
      if (self.isExprOf[X]) self.asInstanceOf[Expr[X]]
      else throw new Unsupported("Expr.asExprOf of an expression of another type")
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
    def unapply[TypeBindings, Tup <: Tuple](scrutinee: Expr[Any])(using pattern: Expr[Any]): Option[Tup] = {
      val n = QuoteHost.matchPattern(QuoteHost.exprId(scrutinee), QuoteHost.exprId(pattern))
      if (n < 0) None
      else {
        val things = new Array[Object](n)
        var i = 0
        while (i < n) {
          val m = QuoteHost.matched(i)
          things(i) = if (m >= 0) QuoteHost.expr(m) else QuoteHost.tpe(-1 - m)
          i += 1
        }
        Some(Tuple.fromArray(things).asInstanceOf[Tup])
      }
    }
  }

  object TypeMatch extends TypeMatchModule {
    def unapply[TypeBindings, Tup <: Tuple](scrutinee: Type[?])(using pattern: Type[?]): Option[Tup] = {
      val n = ReflectOps.one("matchType", QuoteHost.typeId(scrutinee), QuoteHost.typeId(pattern))
      if (n < 0) None
      else Some(Tuple.fromArray(Array.tabulate[Object](n)(i => QuoteHost.tpe(-1 - QuoteHost.matched(i)).asInstanceOf[Object])).asInstanceOf[Tup])
    }
  }

  private def text(pickled: String | List[String]): String = pickled match {
    case s: String       => s
    case l: List[String] => l.mkString
  }

  private def typeIds(types: Seq[Type[?]] | Null): Array[Int] =
    if (types == null) new Array[Int](0) else types.nn.map(QuoteHost.typeId).toArray
}
