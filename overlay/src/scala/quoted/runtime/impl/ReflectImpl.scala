// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.quoted.Expr
import scala.quoted.runtime.StopMacroExpansion
import scala.reflect.TypeTest

/** The `reflect` of `QuotesImpl`. */
trait ReflectSupport { this: QuotesImpl =>

/** The subset of `quotes.reflect` the frontend implements: terms are `ExprImpl`s, types
 *  `TypeImpl`s, symbols `SymbolImpl`s, constants `ConstantImpl`s. The VM allocates it and the
 *  `...MethodsImpl` classes without running a constructor; a member not implemented here is no
 *  method of the class, which refuses the expansion.
 */
abstract class ReflectImpl
    extends reflectModule, ReflectTrees, ReflectTerms, ReflectTypes, ReflectTypeShapes, ReflectConstants, ReflectSymbols, ReflectFlags,
      ReflectDefinitions, ReflectPositions, ReflectControl, ReflectTypeTrees, ReflectStatements,
      ReflectPatterns, ReflectTypeKinds, ReflectLambdaTypes, ReflectTypeBounds {
  self: reflect.type =>

  extension (expr: Expr[Any]) def asTerm: Term = ReflectOps.term(QuoteHost.exprId(expr)).asInstanceOf[Term]

  /** `TreeCode`, `TreeStructure` and `TypeReprCode`; the other printers are `Unsupported`. */
  object Printer extends PrinterModule {
    def TreeCode: Printer[Tree] = printer("showTree")
    def TreeShortCode: Printer[Tree] = refusing("Printer.TreeShortCode")
    def TreeAnsiCode: Printer[Tree] = refusing("Printer.TreeAnsiCode")
    def TreeStructure: Printer[Tree] = printer("showStructure")
    def TypeReprCode: Printer[TypeRepr] = printer("showType")
    def TypeReprShortCode: Printer[TypeRepr] = refusing("Printer.TypeReprShortCode")
    def TypeReprAnsiCode: Printer[TypeRepr] = refusing("Printer.TypeReprAnsiCode")
    def TypeReprStructure: Printer[TypeRepr] = refusing("Printer.TypeReprStructure")
    def ConstantCode: Printer[Constant] = refusing("Printer.ConstantCode")
    def ConstantStructure: Printer[Constant] = refusing("Printer.ConstantStructure")

    private def printer[T](op: String): Printer[T] = new Printer[T] {
      def show(x: T): String = ReflectOps.text(op, ReflectOps.id(x))
    }

    private def refusing[T](what: String): Printer[T] = new Printer[T] {
      def show(x: T): String = throw new Unsupported(what)
    }
  }

  /** Errors are recorded on the frontend, which reports them for the expansion; warnings and
   *  infos are `Unsupported`.
   */
  object report extends reportModule {
    def error(msg: String): Unit = ReflectOps.run("error", Array(-1), msg)
    def error(msg: String, expr: Expr[Any]): Unit = ReflectOps.run("error", Array(QuoteHost.exprId(expr)), msg)
    def error(msg: String, pos: Position): Unit = {
      val p = pos.asInstanceOf[PositionImpl]
      ReflectOps.run("errorAt", Array(p.file, p.start, p.end), msg)
    }
    def errorAndAbort(msg: String): Nothing = { error(msg); throw new StopMacroExpansion }
    def errorAndAbort(msg: String, expr: Expr[Any]): Nothing = { error(msg, expr); throw new StopMacroExpansion }
    def errorAndAbort(msg: String, pos: Position): Nothing = { error(msg, pos); throw new StopMacroExpansion }
    def throwError(msg: String): Nothing = errorAndAbort(msg)
    def throwError(msg: String, expr: Expr[Any]): Nothing = errorAndAbort(msg, expr)
    def throwError(msg: String, pos: Position): Nothing = errorAndAbort(msg, pos)
    def warning(msg: String): Unit = throw new Unsupported("report.warning")
    def warning(msg: String, expr: Expr[Any]): Unit = throw new Unsupported("report.warning")
    def warning(msg: String, pos: Position): Unit = throw new Unsupported("report.warning")
    def info(msg: String): Unit = throw new Unsupported("report.info")
    def info(msg: String, expr: Expr[Any]): Unit = throw new Unsupported("report.info")
    def info(msg: String, pos: Position): Unit = throw new Unsupported("report.info")
  }

  /** Searches the typer answers between runs of the expansion; a failure is one without a
   *  matching candidate.
   */
  object Implicits extends ImplicitsModule {
    def search(tpe: TypeRepr): ImplicitSearchResult = ReflectOps.results("search", ReflectOps.id(tpe)) match {
      case List(1, tree) => new SearchImpl(tree).asInstanceOf[ImplicitSearchResult]
      case _             => new SearchImpl(-1).asInstanceOf[ImplicitSearchResult]
    }
    def searchIgnoring(tpe: TypeRepr)(ignored: Symbol*): ImplicitSearchResult =
      throw new Unsupported("Implicits.searchIgnoring")
  }

  given ImplicitSearchSuccessTypeTest: TypeTest[ImplicitSearchResult, ImplicitSearchSuccess] = searched(true)
  given ImplicitSearchFailureTypeTest: TypeTest[ImplicitSearchResult, ImplicitSearchFailure] = searched(false)
  given NoMatchingImplicitsTypeTest: TypeTest[ImplicitSearchResult, NoMatchingImplicits] = searched(false)
  given ImplicitSearchSuccessMethods: ImplicitSearchSuccessMethods =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectSupport$ReflectImpl$SuccessMethods").asInstanceOf[ImplicitSearchSuccessMethods]
  given ImplicitSearchFailureMethods: ImplicitSearchFailureMethods =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectSupport$ReflectImpl$FailureMethods").asInstanceOf[ImplicitSearchFailureMethods]

  abstract class SuccessMethods extends ImplicitSearchSuccessMethods {
    extension (self: ImplicitSearchSuccess) def tree: Term =
      ReflectOps.term(self.asInstanceOf[SearchImpl].tree).asInstanceOf[Term]
  }

  abstract class FailureMethods extends ImplicitSearchFailureMethods {
    extension (self: ImplicitSearchFailure) def explanation: String = "no implicit argument of the type was found"
  }

  private def searched[T](success: Boolean): T = new SearchTest(success).asInstanceOf[T]
}
}

/** The result of an implicit search: the expression found, `-1` for none. */
final class SearchImpl(val tree: Int)

/** The type test of successful searches if `success`, else of failed ones. */
final class SearchTest(success: Boolean) extends TypeTest[Any, Any] {
  def unapply(x: Any): Option[x.type & Any] = if ((x.asInstanceOf[SearchImpl].tree >= 0) == success) Some(x) else None
}
