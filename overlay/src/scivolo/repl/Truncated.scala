// Derived from PPrint (MIT, see licenses/pprint.txt); translated and changed:
//   pprint/src/pprint/Truncated.scala
// Copyright (c) 2019 Li Haoyi.
package scivolo.repl

import scala.collection.mutable.ArrayBuffer

/** The chunks of `chunks0`, cut off by `...` once the text wrapped at `width` grows beyond
  * `height` lines.
  */
final class Truncated(chunks0: Iterator[String], width: Int, height: Int) extends Iterator[String] {
  private val lineLengths = ArrayBuffer(0)
  private val chunks = chunks0.filter(_.length > 0)
  private var lastLineFinished = false
  private var lastChunkLeftover = ""

  private def handleNormalChar(char: Char): Unit =
    if (char == '\n' || char == '\r') lineLengths += 0
    else if (lineLengths.last == width) lineLengths += 1
    else lineLengths(lineLengths.length - 1) += 1

  private def completedLines = lineLengths.length - 1

  /** Where consuming `chunk` stopped at `lineLimit` completed lines, unless it was consumed whole. */
  private def consumeChunkUntilLine(chunk: String, lineLimit: Int): Option[Int] = {
    var i = 0
    while (i < chunk.length && completedLines < lineLimit) {
      handleNormalChar(chunk.charAt(i))
      i += 1
    }
    if (i == chunk.length) None else Some(i)
  }

  def toResult: Result = new Result(this, lineLengths.length - 1, lineLengths(lineLengths.length - 1))

  def hasNext: Boolean = (chunks.hasNext && completedLines < height - 1) || !lastLineFinished

  def next(): String =
    if (chunks.hasNext && completedLines < height - 1) {
      val chunk = chunks.next()
      consumeChunkUntilLine(chunk, height - 1) match {
        case None =>
          if (!chunks.hasNext) lastLineFinished = true
          chunk
        case Some(i) =>
          val splitPoint = if (lineLengths.last != 0) i - 1 else i
          lastChunkLeftover = chunk.substring(splitPoint, chunk.length)
          chunk.substring(0, splitPoint)
      }
    } else if (!lastLineFinished) {
      val buffer = ArrayBuffer.empty[String]
      var charsLeftOver = false
      consumeChunkUntilLine(lastChunkLeftover, height) match {
        case None => buffer += lastChunkLeftover
        case Some(i) =>
          charsLeftOver = true
          buffer += lastChunkLeftover.substring(0, i - 1)
      }
      while (chunks.hasNext && completedLines < height) {
        val chunk = chunks.next()
        consumeChunkUntilLine(chunk, height) match {
          case None => buffer += chunk
          case Some(i) =>
            charsLeftOver = true
            buffer += chunk.substring(0, i)
        }
      }
      lastLineFinished = true
      if (charsLeftOver || chunks.hasNext) "..." else buffer.mkString
    } else throw new NoSuchElementException("next on empty iterator")
}
