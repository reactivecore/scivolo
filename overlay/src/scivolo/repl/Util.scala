package scivolo.repl

import scala.annotation.tailrec

object Util {
  def concat[T](is: (() => Iterator[T])*): Iterator[T] =
    new ConcatIterator(is.iterator.map(_()), () => Iterator.empty)

  /** The iterators of `it0` with `joiner`'s between each two. */
  def joinIter[T](it0: Iterator[Iterator[T]], joiner: => Iterator[T]): Iterator[T] =
    new ConcatIterator(it0, () => joiner)

  final class ConcatIterator[T](it0: Iterator[Iterator[T]], joiner: () => Iterator[T]) extends Iterator[T] {
    private var head: Iterator[T] = null
    private var count = 0

    @tailrec private def check(): Boolean =
      if (head != null && head.hasNext) true
      else if (!it0.hasNext) false
      else {
        head = if (count % 2 == 0) it0.next() else joiner()
        count += 1
        check()
      }

    def hasNext: Boolean = check()

    def next(): T = {
      check()
      head.next()
    }
  }

  def isOperator(ident: String): Boolean =
    ident.nonEmpty && "<~!@#%^*+->?:=&|/\\".indexOf(ident.charAt(0)) >= 0

  def isIdentifier(name: String): Boolean = {
    def isStart(c: Char) = c == '_' || c == '$' || Character.isUnicodeIdentifierStart(c)
    def isPart(c: Char) = c == '$' || Character.isUnicodeIdentifierPart(c)
    name.nonEmpty && isStart(name.charAt(0)) && name.drop(1).forall(isPart)
  }

  def escapeChar(c: Char, sb: StringBuilder): Unit = c match {
    case '"'  => sb.append("\\\"")
    case '\\' => sb.append("\\\\")
    case '\b' => sb.append("\\b")
    case '\f' => sb.append("\\f")
    case '\n' => sb.append("\\n")
    case '\r' => sb.append("\\r")
    case '\t' => sb.append("\\t")
    case c if c < ' ' =>
      val hex = Integer.toHexString(c.toInt)
      sb.append("\\u").append("0" * (4 - hex.length)).append(hex)
    case c => sb.append(c)
  }

  /** `s` as a string literal. */
  def literalize(s: String): String = {
    val sb = new StringBuilder
    sb.append('"')
    s.foreach(escapeChar(_, sb))
    sb.append('"')
    sb.toString
  }
}
