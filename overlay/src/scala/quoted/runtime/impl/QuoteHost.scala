package scala.quoted.runtime.impl

import scala.quoted.{Expr, Quotes, Type}

/** The frontend's quotes as a macro sees them: each `Expr` and `Type` a number the frontend
 *  gave its tree or type.
 */
object QuoteHost {

  /** The quote pickled as `pickled`, its type holes filled with `types`; its term holes open. */
  def unpickleExpr(pickled: String, types: Array[Int]): Int = throw new UnsupportedOperationException("VM native")

  /** The type pickled as `pickled`, its type holes filled with `types`. */
  def unpickleType(pickled: String, types: Array[Int]): Int = throw new UnsupportedOperationException("VM native")

  def holeCount(quote: Int): Int = throw new UnsupportedOperationException("VM native")

  /** The index the quote's pickle gives term hole `hole`. */
  def holeIndex(quote: Int, hole: Int): Int = throw new UnsupportedOperationException("VM native")

  def holeArgCount(quote: Int, hole: Int): Int = throw new UnsupportedOperationException("VM native")

  /** Argument `arg` of term hole `hole`: an `Expr` number, or `-1 - n` for `Type` number `n`. */
  def holeArg(quote: Int, hole: Int, arg: Int): Int = throw new UnsupportedOperationException("VM native")

  def fillHole(quote: Int, hole: Int, expr: Int): Unit = throw new UnsupportedOperationException("VM native")

  def expr(id: Int): Expr[Any] = new ExprImpl(id)

  def tpe(id: Int): Type[?] = new TypeImpl(id)

  def quotes(): Quotes = new QuotesImpl

  def exprId(e: Expr[?]): Int = e match {
    case e: ExprImpl => e.id
    case _           => throw new Unsupported("an Expr made outside the frontend")
  }

  def typeId(t: Type[?]): Int = t match {
    case t: TypeImpl => t.id
    case _           => throw new Unsupported("a Type made outside the frontend")
  }
}

/** An operation of `Quotes` the frontend does not implement. */
final class Unsupported(what: String) extends RuntimeException(what)

final class ExprImpl(val id: Int) extends Expr[Any]

final class TypeImpl(val id: Int) extends Type[Nothing]
