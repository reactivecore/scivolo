package scalai.repl

import scala.collection.mutable.ArrayBuffer

/** Rendered text, and the lines it completes and the length of its last line once `iter` is
  * exhausted.
  */
final class Result(val iter: Iterator[String], completedLineCount0: => Int, lastLineLength0: => Int) {
  lazy val completedLineCount: Int = {
    require(iter.isEmpty)
    completedLineCount0
  }

  lazy val lastLineLength: Int = {
    require(iter.isEmpty)
    lastLineLength0
  }

  def flatMap(f: (Int, Int) => Result): Result = {
    var newCompletedLineCount = 0
    var newLastLineLength = 0
    val merged = Util.concat(
      () => iter,
      () => {
        require(!iter.hasNext)
        val newResult = f(completedLineCount, lastLineLength0)
        newResult.iter.map { x =>
          if (!newResult.iter.hasNext) {
            newCompletedLineCount = newResult.completedLineCount
            newLastLineLength = newResult.lastLineLength
          }
          x
        }
      }
    )
    new Result(
      merged,
      newCompletedLineCount + completedLineCount,
      if (newCompletedLineCount > 0) newLastLineLength else newLastLineLength + lastLineLength
    )
  }
}

object Result {
  def fromString(s: => String): Result = {
    lazy val lines = s.linesIterator.toArray
    new Result(Iterator(s), lines.length - 1, lines.last.length)
  }
}

/** Trees as text: an `Apply` on one line if it fits into `maxWidth`, else one child per line. */
final class Renderer(maxWidth: Int, indentStep: Int) {
  private def indent(n: Int) = " " * n

  def rec(x: Tree, leftOffset: Int, indentCount: Int): Result = x match {
    case Tree.Apply(prefix, body) =>
      val nonEmpty = body.hasNext
      var totalHorizontalWidth = leftOffset + prefix.length + 2
      val buffer = ArrayBuffer.empty[Seq[String]]
      var lastChildIter = Iterator[String]()
      var childCompletedLineCount = 0
      while (body.hasNext && totalHorizontalWidth <= maxWidth && childCompletedLineCount == 0) {
        val child = body.next()
        val childRes = rec(child, (indentCount + 1) * indentStep, indentCount + 1)
        val childBuffer = ArrayBuffer.empty[String]
        while (childRes.iter.hasNext && totalHorizontalWidth < maxWidth) {
          val next = childRes.iter.next()
          childBuffer += next
          totalHorizontalWidth += next.length
        }
        if (body.hasNext) totalHorizontalWidth += 2
        if (!childRes.iter.hasNext) childCompletedLineCount += childRes.completedLineCount
        else lastChildIter = childRes.iter
        buffer += childBuffer.toSeq
      }

      def applyHeader = Iterator(prefix, "(")
      val indentPlusOne = indent((indentCount + 1) * indentStep)
      def separator = Iterator(",\n", indentPlusOne)

      if (totalHorizontalWidth <= maxWidth && childCompletedLineCount == 0 && !lastChildIter.hasNext) {
        val iter = Util.concat(
          () => applyHeader,
          () => Util.joinIter(buffer.iterator.map(_.iterator), Iterator(", ")),
          () => Iterator(")")
        )
        new Result(iter, 0, buffer.iterator.map(_.iterator.map(_.length).sum).sum)
      } else if (!nonEmpty && totalHorizontalWidth > maxWidth) {
        val iter = Util.concat(() => applyHeader, () => Iterator("\n", indent(indentCount * indentStep), ")"))
        new Result(iter, 0, buffer.iterator.map(_.iterator.map(_.length).sum).sum)
      } else {
        def bufferedFragments = Util.joinIter(
          for ((v, i) <- buffer.iterator.zipWithIndex)
            yield if (i < buffer.length - 1) v.iterator else v.iterator ++ lastChildIter,
          separator
        )
        def nonBufferedFragments =
          Util.joinIter(body.map(c => rec(c, (indentCount + 1) * indentStep, indentCount + 1).iter), separator)
        def allFragments =
          if (buffer.isEmpty) nonBufferedFragments
          else if (!body.hasNext) bufferedFragments
          else Util.joinIter(Iterator(bufferedFragments, nonBufferedFragments), separator)
        val iter = Util.concat(
          () => applyHeader,
          () => Iterator("\n", indentPlusOne),
          () => allFragments,
          () => Iterator("\n", indent(indentCount * indentStep), ")")
        )
        new Result(iter, childCompletedLineCount + 2, indentCount * indentStep + 1)
      }

    case Tree.Infix(lhs, op, rhs) =>
      rec(lhs, leftOffset, indentCount).flatMap { (_, lhsLastLineLength) =>
        Result.fromString(" " + op + " ").flatMap((_, _) => rec(rhs, lhsLastLineLength, indentCount))
      }

    case Tree.Lazy(body) => new Truncated(Iterator(body()), maxWidth, 99999999).toResult

    case Tree.Literal(body) => Result.fromString(body)

    case Tree.KeyValue(k, v) =>
      val prefix = s"$k = "
      Result.fromString(prefix).flatMap((_, _) => rec(v, leftOffset + prefix.length, indentCount))
  }
}
