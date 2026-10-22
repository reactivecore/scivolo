// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.quoted.Expr

/** The `reflect` of `QuotesImpl`. */
trait ReflectSupport { this: QuotesImpl =>

/** The subset of `quotes.reflect` the frontend implements: terms are `ExprImpl`s, types
 *  `TypeImpl`s, symbols `SymbolImpl`s, constants `ConstantImpl`s. The VM allocates it and the
 *  `...MethodsImpl` classes without running a constructor; a member not implemented here is no
 *  method of the class, which refuses the expansion.
 */
abstract class ReflectImpl
    extends reflectModule, ReflectTerms, ReflectTypes, ReflectConstants, ReflectSymbols, ReflectFlags {
  self: reflect.type =>

  extension (expr: Expr[Any]) def asTerm: Term = ReflectOps.term(QuoteHost.exprId(expr)).asInstanceOf[Term]
}
}
