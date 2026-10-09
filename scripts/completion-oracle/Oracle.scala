//> using scala 3.9.0
//> using dep org.scala-lang::scala3-repl:3.9.0

// Rewrites completion corpora (`*.tab`, format in docs/research/11-completion.md) in place with
// the labels dotty's REPL lists on Tab, one fresh session per file.

import dotty.tools.dotc.config.Properties
import dotty.tools.dotc.core.Contexts.Context
import dotty.tools.repl.{JLineTerminal, ReplDriver, State}
import java.io.{ByteArrayOutputStream, File, InputStream, OutputStream, PrintStream}
import java.nio.charset.StandardCharsets.UTF_8
import java.nio.file.{Files, Path}
import java.util.Locale
import org.jline.reader.{Candidate, CompletingParsedLine, LineReader, ParsedLine, Parser}
import org.jline.reader.Parser.ParseContext
import org.jline.reader.impl.{CompletionMatcherImpl, LineReaderImpl}
import org.jline.terminal.impl.DumbTerminal
import scala.collection.mutable.ListBuffer
import scala.jdk.CollectionConverters.*

val Jdk = 21
val Header = "// oracle:"
val Prompt = "scala> "
val Tab = "tab> "
val Miss = "miss> "
val Continuation = "     |"
val Answer = "  "
val Cursor = "@@"
val Width = 100

class Probe(settings: Array[String], out: PrintStream) extends ReplDriver(settings, out):
  def labels(cursor: Int, expr: String, state: State): List[String] =
    completions(cursor, expr, state).map(_.label).distinct

/** What JLine, set up as `JLineTerminal.readLine` sets it up, lists for the REPL's labels. */
object Shown:
  private val terminal =
    JLineTerminal(DumbTerminal(InputStream.nullInputStream, OutputStream.nullOutputStream))
  private val options = Map(
    LineReader.Option.INSERT_TAB -> true,
    LineReader.Option.AUTO_FRESH_LINE -> true,
    LineReader.Option.DISABLE_EVENT_EXPANSION -> true
  ).map((k, v) => k -> java.lang.Boolean.valueOf(v)).asJava

  private def parser(using Context): Parser =
    val parser = Class.forName(classOf[JLineTerminal].getName + "$Parser")
    parser.getDeclaredConstructors.head.newInstance(terminal, summon[Context]).asInstanceOf[Parser]

  private def insertsTab(line: String) = line.matches("(^|[\\s\\S]*\n)[\r\n\t ]*")

  def apply(line: String, cursor: Int, labels: List[String])(using Context): List[String] =
    if insertsTab(line) then Nil
    else
      val parsed = parser.parse(line, cursor, ParseContext.COMPLETE)
      val candidates = labels.map(l => Candidate(l, l, null, null, null, null, false))
      val matcher = CompletionMatcherImpl()
      val group = LineReaderImpl.DEFAULT_ORIGINAL_GROUP_NAME
      matcher.compile(options, false, Wrapped(parsed), false, LineReaderImpl.DEFAULT_ERRORS, group)
      matcher.matches(candidates.asJava).asScala.toList.sorted.map(_.value)

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

enum Entry:
  case Kept(line: String)
  case Run(lines: List[String])
  case Ask(lines: List[String])
  case Misses(line: String)

def entries(lines: List[String], file: Path): List[Entry] =
  def group(first: String, rest: List[String]) =
    val (more, after) = rest.span(_.startsWith(Continuation))
    (first :: more, after)
  lines match
    case Nil => Nil
    case first :: rest if first.startsWith(Header) || first.startsWith(Answer) =>
      entries(rest, file)
    case first :: rest if first.startsWith(Prompt) =>
      val (input, after) = group(first, rest)
      Entry.Run(input) :: entries(after, file)
    case first :: rest if first.startsWith(Tab) =>
      val (input, after) = group(first, rest)
      Entry.Ask(input) :: entries(after, file)
    case first :: rest if first.startsWith(Miss) => Entry.Misses(first) :: entries(rest, file)
    case first :: rest if first.isEmpty || first.startsWith("//") =>
      Entry.Kept(first) :: entries(rest, file)
    case first :: _ => sys.error(s"$file: unexpected line: $first")

def text(lines: List[String], prompt: String): String =
  val more = lines.tail.map(_.stripPrefix(Continuation).stripPrefix(" "))
  (lines.head.stripPrefix(prompt) :: more).mkString("\n")

def wrap(labels: List[String]): List[String] =
  if labels.isEmpty then List(Answer + "<none>")
  else
    labels.foldLeft(List.empty[String]):
      case (line :: done, label) if line.length + 1 + label.length <= Width =>
        s"$line $label" :: done
      case (done, label) => (Answer + label) :: done
    .reverse

def libraryPath: String =
  System.getProperty("java.class.path").split(File.pathSeparator).filter: entry =>
    val name = Path.of(entry).getFileName.toString
    name.startsWith("scala-library") || name.startsWith("scala3-library")
  .mkString(File.pathSeparator)

class Session(file: Path):
  private val bytes = ByteArrayOutputStream()
  private val driver = Probe(
    Array("-classpath", libraryPath, "-color:never"),
    PrintStream(bytes, true, UTF_8)
  )
  private var state: State = driver.initialState
  private var answer = Option.empty[List[String]]
  val errors = ListBuffer.empty[String]

  def run(input: String): Unit =
    state = driver.run(input)(using state)
    val out = bytes.toString(UTF_8)
    bytes.reset()
    answer = None
    if out.linesIterator.exists(l => l.startsWith("-- ") && l.contains("Error")) then
      errors += s"`$input` fails:\n$out"

  def ask(input: String): List[String] =
    val cursor = input.indexOf(Cursor)
    if cursor < 0 then sys.error(s"$file: no $Cursor in `$input`")
    val line = input.patch(cursor, "", Cursor.length)
    val shown = Shown(line, cursor, driver.labels(cursor, line, state))(using state.context)
    def splits(l: String) = !l.startsWith("`") && l.exists(_.isWhitespace)
    shown.find(l => l.startsWith("<Error") || splits(l) || l.contains('\n')).foreach: l =>
      errors += s"`$input` gives the label `$l`"
    answer = Some(shown)
    shown

  def misses(line: String): Unit = answer match
    case None => errors += s"`$line` follows no answer"
    case Some(shown) =>
      val extra = line.stripPrefix(Miss).split(' ').filter(_.nonEmpty).filterNot(shown.contains)
      if extra.nonEmpty then errors += s"`$line` lists labels not shown: ${extra.mkString(" ")}"

  def kept(line: String): Unit = if line.isEmpty then answer = None

def regenerate(file: Path, header: String): (Int, List[String]) =
  val session = Session(file)
  var asked = 0
  val out = entries(Files.readAllLines(file, UTF_8).asScala.toList, file).flatMap:
    case Entry.Kept(line) =>
      session.kept(line)
      List(line)
    case Entry.Run(lines) =>
      session.run(text(lines, Prompt))
      lines
    case Entry.Ask(lines) =>
      asked += 1
      lines ++ wrap(session.ask(text(lines, Tab)))
    case Entry.Misses(line) =>
      session.misses(line)
      List(line)
  if session.errors.isEmpty then
    Files.writeString(file, (header :: out).mkString("", "\n", "\n"), UTF_8)
  (asked, session.errors.toList)

@main def oracle(files: String*): Unit =
  val jdk = Runtime.version.feature
  if jdk != Jdk then sys.error(s"the oracle runs on JDK $Jdk, not $jdk")
  val header = s"$Header scala3-repl ${Properties.versionNumberString}, JDK $jdk"
  val failed = files.filter: name =>
    val start = System.nanoTime
    val (asked, errors) = regenerate(Path.of(name), header)
    val seconds = "%.1f".formatLocal(Locale.ROOT, (System.nanoTime - start) / 1e9)
    println(s"$name: $asked completions, $seconds s")
    errors.foreach(e => System.err.println(s"$name: $e"))
    errors.nonEmpty
  if failed.nonEmpty then
    System.err.println(s"not rewritten: ${failed.mkString(" ")}")
    sys.exit(1)
