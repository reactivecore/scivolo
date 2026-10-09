//> using scala 3.9.0
//> using dep org.scala-lang::scala3-repl:3.9.0

// Rewrites completion corpora (`*.tab`, format in docs/research/11-completion.md) in place with
// what dotty's REPL lists on Tab, one fresh session per file.

import dotty.tools.dotc.config.Properties
import dotty.tools.repl.{ReplDriver, State}
import java.io.{ByteArrayOutputStream, File, PrintStream}
import java.nio.charset.StandardCharsets.UTF_8
import java.nio.file.{Files, Path}
import java.util.Locale
import org.jline.reader.Parser
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
val Thrown = """^[\w.$]+(Exception|Error|Throwable)(: .*)?$""".r

class Probe(settings: Array[String], out: PrintStream) extends ReplDriver(settings, out):
  def labels(cursor: Int, expr: String, state: State): List[String] =
    completions(cursor, expr, state).map(_.label).distinct

enum Entry:
  case Kept(line: String)
  case Run(lines: List[String])
  case Ask(lines: List[String])
  case Misses(line: String)

def isAnswer(line: String) = line.startsWith(Answer) && line.lift(Answer.length).exists(_ != ' ')

def entries(lines: List[String], file: Path): List[Entry] =
  def group(first: String, rest: List[String]) =
    val (more, after) = rest.span(_.startsWith(Continuation))
    (first :: more, after)
  lines match
    case Nil => Nil
    case first :: rest if first.startsWith(Header) || isAnswer(first) => entries(rest, file)
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

def wrap(groups: List[Group]): List[String] =
  if groups.isEmpty then List(Answer + "<none>")
  else
    groups.flatMap: group =>
      (group.header.map(h => s"[$h]").toList ++ group.labels).foldLeft(List.empty[String]):
        case (line :: done, token) if line.length + 1 + token.length <= Width =>
          s"$line $token" :: done
        case (done, token) => (Answer + token) :: done
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
  private var parser: Parser = Shown.parser(using state.context)
  private var answer = Option.empty[List[String]]
  val errors = ListBuffer.empty[String]

  def run(input: String): Unit =
    state = driver.run(input)(using state)
    parser = Shown.parser(using state.context)
    val out = bytes.toString(UTF_8)
    bytes.reset()
    answer = None
    def fails(l: String) = l.startsWith("-- ") && l.contains("Error") || Thrown.matches(l)
    if out.linesIterator.exists(fails) then errors += s"`$input` fails:\n$out"

  def ask(input: String): List[Group] =
    val cursor = input.indexOf(Cursor)
    if cursor < 0 then sys.error(s"$file: no $Cursor in `$input`")
    val line = input.patch(cursor, "", Cursor.length)
    val labels = driver.labels(cursor, line, state)
    def splits(l: String) = !l.startsWith("`") && l.exists(_.isWhitespace)
    labels.find(l => l.startsWith("<Error") || splits(l) || l.contains('\n')).foreach: l =>
      errors += s"`$input` gives the label `$l`"
    val shown = Shown(parser, line, cursor, labels)
    answer = Some(shown.flatMap(_.labels))
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
