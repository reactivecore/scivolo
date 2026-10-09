//> using scala 3.9.0
//> using dep org.scala-lang::scala3-repl:3.9.0

// Rewrites completion corpora (`*.tab`, format in docs/research/11-completion.md) in place with
// the labels dotty's REPL offers, one fresh session per file.

import dotty.tools.dotc.config.Properties
import dotty.tools.repl.{ReplDriver, State}
import java.io.{ByteArrayOutputStream, File, PrintStream}
import java.nio.charset.StandardCharsets.UTF_8
import java.nio.file.{Files, Path}
import java.util.Locale
import scala.jdk.CollectionConverters.*

val Jdk = "21"
val Header = "// oracle:"
val Prompt = "scala> "
val Tab = "tab> "
val Miss = "miss> "
val Continuation = "     | "
val Answer = "  "
val Cursor = "@@"
val Width = 100

class Probe(settings: Array[String], out: PrintStream) extends ReplDriver(settings, out):
  def labels(cursor: Int, expr: String, state: State): List[String] =
    completions(cursor, expr, state).map(_.label).distinct.sorted

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
  (lines.head.stripPrefix(prompt) :: lines.tail.map(_.stripPrefix(Continuation))).mkString("\n")

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
  private var answer = List.empty[String]

  def run(input: String): Unit =
    state = driver.run(input)(using state)
    val out = bytes.toString(UTF_8)
    bytes.reset()
    if out.linesIterator.exists(l => l.startsWith("-- ") && l.contains("Error")) then
      System.err.println(s"$file: `$input` fails:\n$out")

  def ask(input: String): List[String] =
    val cursor = input.indexOf(Cursor)
    if cursor < 0 then sys.error(s"$file: no $Cursor in `$input`")
    answer = driver.labels(cursor, input.patch(cursor, "", Cursor.length), state)
    def splits(l: String) = !l.startsWith("`") && l.exists(_.isWhitespace)
    answer.find(l => l.startsWith("<Error") || splits(l) || l.contains('\n')).foreach: l =>
      sys.error(s"$file: `$input` gives the label `$l`")
    answer

  def checkMisses(line: String): Unit =
    val extra = line.stripPrefix(Miss).split(' ').filter(_.nonEmpty).filterNot(answer.contains)
    if extra.nonEmpty then
      System.err.println(s"$file: `$line` lists labels dotty does not offer: ${extra.mkString(" ")}")

def regenerate(file: Path, header: String): Int =
  val session = Session(file)
  var asked = 0
  val out = entries(Files.readAllLines(file, UTF_8).asScala.toList, file).flatMap:
    case Entry.Kept(line) => List(line)
    case Entry.Run(lines) =>
      session.run(text(lines, Prompt))
      lines
    case Entry.Ask(lines) =>
      asked += 1
      lines ++ wrap(session.ask(text(lines, Tab)))
    case Entry.Misses(line) =>
      session.checkMisses(line)
      List(line)
  Files.writeString(file, (header :: out).mkString("", "\n", "\n"), UTF_8)
  asked

@main def oracle(files: String*): Unit =
  val jdk = System.getProperty("java.version")
  if !jdk.startsWith(Jdk + ".") then sys.error(s"the oracle runs on JDK $Jdk, not $jdk")
  val header = s"$Header scala3-repl ${Properties.versionNumberString}, JDK $jdk"
  for name <- files do
    val start = System.nanoTime
    val asked = regenerate(Path.of(name), header)
    val seconds = "%.1f".formatLocal(Locale.ROOT, (System.nanoTime - start) / 1e9)
    println(s"$name: $asked completions, $seconds s")
