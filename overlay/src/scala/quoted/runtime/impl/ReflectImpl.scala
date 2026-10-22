// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.quoted.Expr
import scala.quoted.runtime.StopMacroExpansion

/** The `reflect` of `QuotesImpl`. */
trait ReflectSupport { this: QuotesImpl =>

/** The subset of `quotes.reflect` the frontend implements: terms are `ExprImpl`s, types
 *  `TypeImpl`s, symbols `SymbolImpl`s, constants `ConstantImpl`s. The VM allocates it and the
 *  `...MethodsImpl` classes without running a constructor; a member not implemented here is no
 *  method of the class, which refuses the expansion.
 */
abstract class ReflectImpl
    extends reflectModule, ReflectTrees, ReflectTerms, ReflectTypes, ReflectTypeShapes, ReflectConstants, ReflectSymbols, ReflectFlags {
  self: reflect.type =>

  extension (expr: Expr[Any]) def asTerm: Term = ReflectOps.term(QuoteHost.exprId(expr)).asInstanceOf[Term]

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
}
}
