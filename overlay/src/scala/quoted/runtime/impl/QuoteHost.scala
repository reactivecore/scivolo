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
   *  `n`, `Int.MinValue` for the `Quotes`, `Int.MinValue + 1` for a literal, `Int.MinValue + 2`
   *  for a `Seq` of the arguments after it, `Int.MinValue + 3` for a `Function0` of the one after it.
   */
  def arg(arg: Int): Int = throw new UnsupportedOperationException("VM native")

  /** How many elements the `Seq` of argument `arg` has. */
  def elementCount(arg: Int): Int = throw new UnsupportedOperationException("VM native")

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
    val quotes = new QuotesImpl
    var made = List.empty[Object]
    var i = 0
    while (i < argCount()) {
      val (value, next) = this.value(i, quotes)
      made = value() :: made
      i = next
    }
    made.reverse.toArray
  }

  /** The maker of the value of argument `i`, each `Expr`, `Type` and `Seq` fresh, and the index of
   *  the argument after it and those it holds.
   */
  private def value(i: Int, quotes: Quotes): (() => Object, Int) = {
    val n = arg(i)
    if (n == Int.MinValue + 2) {
      var (elems, next, left) = (List.empty[() => Object], i + 1, elementCount(i))
      while (left > 0) {
        val (elem, after) = value(next, quotes)
        elems = elem :: elems
        next = after
        left -= 1
      }
      val makers = elems.reverse
      (() => makers.map(_()), next)
    } else if (n == Int.MinValue + 3) {
      val (inner, next) = value(i + 1, quotes)
      val thunk: Function0[Object] = () => inner()
      (() => thunk, next)
    } else if (n == Int.MinValue) {
      (() => quotes, i + 1)
    } else if (n == Int.MinValue + 1) {
      val made = literal(i)
      (() => made, i + 1)
    } else if (n >= 0) {
      (() => expr(n), i + 1)
    } else {
      (() => quotedType(-1 - n), i + 1)
    }
  }

  /** Runs `transform` of macro annotation `annotation` on expression `definition` and
   *  `companion` (`-1` for none) and hands the definitions it returns to the frontend.
   */
  @scala.annotation.experimental
  def annotate(annotation: Object, definition: Int, companion: Int): Int = {
    val q = new QuotesImpl
    val tree = this.tree(definition).asInstanceOf[q.reflect.Definition]
    val other = if (companion < 0) None else Some(this.tree(companion).asInstanceOf[q.reflect.Definition])
    val made = annotation.asInstanceOf[scala.annotation.MacroAnnotation].transform(using q)(tree, other)
    ReflectOps.run("definitions", made.map(ReflectOps.id).toArray, null)
  }

  def expr(id: Int): Expr[Any] = new ExprImpl(id, expansion(), false)

  /** Expression `id` as a `quotes.reflect` tree. */
  def tree(id: Int): Expr[Any] = new ExprImpl(id, expansion(), true)

  def quotedType(id: Int): Type[?] = new TypeImpl(id, expansion(), false)

  /** Type `id` as a `TypeRepr`. */
  def tpe(id: Int): Type[?] = new TypeImpl(id, expansion(), true)

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

/** An `Expr`, or a `Tree` if `tree`. */
final class ExprImpl(val id: Int, val expansion: Int, val tree: Boolean) extends Expr[Any] {
  override def toString: String = if (tree) throw new Unsupported("Tree.toString") else super.toString
}

/** A `Type`, or a `TypeRepr` if `repr`. */
final class TypeImpl(val id: Int, val expansion: Int, val repr: Boolean) extends Type[Nothing] {
  override def toString: String =
    if (repr) ReflectOps.text("typeString", QuoteHost.typeId(this)) else super.toString
}
