// What JLine, set up as `JLineTerminal.readLine` sets it up, lists for the REPL's labels.

import dotty.tools.dotc.core.Contexts.Context
import dotty.tools.repl.JLineTerminal
import java.io.{InputStream, OutputStream}
import org.jline.reader.{Candidate, CompletingParsedLine, ParsedLine, Parser}
import org.jline.reader.LineReader.Option as Opt
import org.jline.reader.Parser.ParseContext
import org.jline.reader.impl.LineReaderImpl
import org.jline.terminal.Terminal
import org.jline.terminal.impl.DumbTerminal
import scala.jdk.CollectionConverters.*
import scala.util.control.NonFatal

case class Group(header: Option[String], labels: List[String])

object Shown:
  private val terminal = DumbTerminal(InputStream.nullInputStream, OutputStream.nullOutputStream)
  private val repl = internals(JLineTerminal(terminal))
  private val reader = Reader(terminal)

  private def internals[A](a: => A): A =
    try a
    catch case NonFatal(e) => sys.error(s"scala3-repl internals changed: $e")

  def parser(using ctx: Context): Parser = internals:
    val parser = Class.forName(classOf[JLineTerminal].getName + "$Parser")
    parser.getDeclaredConstructors.head.newInstance(repl, ctx).asInstanceOf[Parser]

  def apply(parser: Parser, line: String, cursor: Int, labels: List[String]): List[Group] =
    if insertsTab(line) then Nil
    else reader.listed(parser.parse(line, cursor, ParseContext.COMPLETE), labels)

  private def insertsTab(line: String) = line.matches("(^|[\\s\\S]*\n)[\r\n\t ]*")

  private class Reader(terminal: Terminal) extends LineReaderImpl(terminal):
    option(Opt.INSERT_TAB, true)
    option(Opt.AUTO_FRESH_LINE, true)
    option(Opt.DISABLE_EVENT_EXPANSION, true)

    def listed(parsed: ParsedLine, labels: List[String]): List[Group] =
      val candidates = labels.map(l => Candidate(l, l, null, null, null, null, false))
      val errors = LineReaderImpl.DEFAULT_ERRORS
      val caseInsensitive = isSet(Opt.CASE_INSENSITIVE)
      completionMatcher.compile(options, false, Wrapped(parsed), caseInsensitive, errors,
        getOriginalGroupName)
      val possible = completionMatcher.matches(candidates.asJava)
      val ordered = java.util.ArrayList[Candidate]()
      val width = (s: String) => Integer.valueOf(s.length)
      val (auto, grouped) = (isSet(Opt.AUTO_GROUP), isSet(Opt.GROUP))
      computePost(possible, null, ordered, parsed.word, width.apply, 1000, auto, grouped,
        isSet(Opt.LIST_ROWS_FIRST))
      val groups = ordered.asScala.toList.groupBy(c => Option(c.group).getOrElse(""))
      val order = ordered.asScala.map(c => Option(c.group).getOrElse("")).distinct.toList
      order.map: group =>
        val name = if group.isEmpty && order.size > 1 then getOthersGroupName else group
        Group(Option.when(auto && grouped && name.nonEmpty)(name), groups(group).map(_.value))

  private class Wrapped(p: ParsedLine) extends CompletingParsedLine:
    def word = p.word
    def wordCursor = p.wordCursor
    def wordIndex = p.wordIndex
    def words = p.words
    def line = p.line
    def cursor = p.cursor
    def escape(candidate: CharSequence, complete: Boolean) = candidate
    def rawWordCursor = p.wordCursor
    def rawWordLength = p.word.length
