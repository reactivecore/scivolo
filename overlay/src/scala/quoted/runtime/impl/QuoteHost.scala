package scala.quoted.runtime.impl

import scala.quoted.{Expr, Quotes, Type}

/** The frontend's quotes as a macro sees them: each `Expr` and `Type` a number the frontend
 *  gave its tree or type in the expansion running.
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

  /** Matches expression `scrutinee` against quote pattern `pattern`: how many things it
   *  matched, `-1` if it does not match.
   */
  def matchPattern(scrutinee: Int, pattern: Int): Int = throw new UnsupportedOperationException("VM native")

  /** Thing `i` the last match matched: an `Expr` number, or `-1 - n` for `Type` number `n`. */
  def matched(i: Int): Int = throw new UnsupportedOperationException("VM native")

  /** Runs `quotes.reflect` operation `op` on `args` and `text`: how many numbers it answers. */
  def reflect(op: String, args: Array[Int], text: String): Int = throw new UnsupportedOperationException("VM native")

  /** Number `i` the last `quotes.reflect` operation answered. */
  def result(i: Int): Int = throw new UnsupportedOperationException("VM native")

  /** The text the last `quotes.reflect` operation answered, if any. */
  def resultText(): String = throw new UnsupportedOperationException("VM native")

  /** An object of class `name`, its constructor not run. */
  def allocate(name: String): Object = throw new UnsupportedOperationException("VM native")

  /** The number of the macro expansion running. */
  def expansion(): Int = throw new UnsupportedOperationException("VM native")

  def argCount(): Int = throw new UnsupportedOperationException("VM native")

  /** Argument `arg` of the macro implementation: an `Expr` number, `-1 - n` for `Type` number
   *  `n`, `Int.MinValue` for the `Quotes`, `Int.MinValue + 1` for a literal.
   */
  def arg(arg: Int): Int = throw new UnsupportedOperationException("VM native")

  /** The descriptor letter of the type of literal argument `arg`: `T` a string, `N` null. */
  def literalTag(arg: Int): Int = throw new UnsupportedOperationException("VM native")

  /** The bits of the value of literal argument `arg`. */
  def literalBits(arg: Int): Long = throw new UnsupportedOperationException("VM native")

  /** The text of string literal argument `arg`. */
  def literalText(arg: Int): String = throw new UnsupportedOperationException("VM native")

  /** Literal argument `arg` boxed; `null` for one the VM passes unboxed. */
  def literal(arg: Int): Object = {
    val bits = literalBits(arg)
    literalTag(arg).toChar match {
      case 'T' => literalText(arg)
      case 'V' => scala.runtime.BoxedUnit.UNIT
      case 'Z' => java.lang.Boolean.valueOf(bits != 0)
      case 'B' => java.lang.Byte.valueOf(bits.toByte)
      case 'S' => java.lang.Short.valueOf(bits.toShort)
      case 'C' => java.lang.Character.valueOf(bits.toChar)
      case 'I' => java.lang.Integer.valueOf(bits.toInt)
      case 'J' => java.lang.Long.valueOf(bits)
      case 'F' => java.lang.Float.valueOf(java.lang.Float.intBitsToFloat(bits.toInt))
      case 'D' => java.lang.Double.valueOf(java.lang.Double.longBitsToDouble(bits))
      case _   => null
    }
  }

  /** Refuses the macro expansion running for `what`. */
  def refuse(what: String): Unit = throw new UnsupportedOperationException("VM native")

  /** The arguments of the macro implementation. */
  def args(): Array[Object] = {
    val args = new Array[Object](argCount())
    var i = 0
    while (i < args.length) {
      val n = arg(i)
      args(i) =
        if (n == Int.MinValue) new QuotesImpl
        else if (n == Int.MinValue + 1) literal(i)
        else if (n >= 0) expr(n)
        else tpe(-1 - n)
      i += 1
    }
    args
  }

  def expr(id: Int): Expr[Any] = new ExprImpl(id, expansion())

  def tpe(id: Int): Type[?] = new TypeImpl(id, expansion())

  def exprId(e: Expr[?]): Int = e match {
    case e: ExprImpl if e.expansion == expansion() => e.id
    case _: ExprImpl                               => throw new Unsupported("an Expr of another macro expansion")
    case _                                         => throw new Unsupported("an Expr made outside the frontend")
  }

  def typeId(t: Type[?]): Int = t match {
    case t: TypeImpl if t.expansion == expansion() => t.id
    case _: TypeImpl                               => throw new Unsupported("a Type of another macro expansion")
    case _                                         => throw new Unsupported("a Type made outside the frontend")
  }
}

/** An operation of `Quotes` the frontend does not implement; it refuses the expansion running
 *  also when the macro catches it.
 */
final class Unsupported(what: String) extends RuntimeException(what) {
  QuoteHost.refuse(what)
}

final class ExprImpl(val id: Int, val expansion: Int) extends Expr[Any] {
  override def toString: String = throw new Unsupported("Expr.toString, Tree.toString")
}

final class TypeImpl(val id: Int, val expansion: Int) extends Type[Nothing] {
  override def toString: String = throw new Unsupported("Type.toString, TypeRepr.toString")
}
