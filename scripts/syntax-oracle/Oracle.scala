//> using scala 3.9.0
//> using dep org.scala-lang::scala3-compiler:3.9.0

package dotty.tools.dotc.syntaxoracle

import dotty.tools.dotc.*
import dotty.tools.dotc.core.Contexts.*
import dotty.tools.dotc.core.Mode
import dotty.tools.dotc.core.Symbols.defn
import dotty.tools.dotc.parsing.Parsers
import dotty.tools.dotc.parsing.Scanners.Scanner
import dotty.tools.dotc.parsing.Tokens.*
import dotty.tools.dotc.reporting.{HideNonSensicalMessages, StoreReporter, UniqueMessagePositions}
import dotty.tools.dotc.util.SourceFile
import dotty.tools.io.AbstractFile
import java.nio.file.{Files, Path, Paths}
import scala.jdk.CollectionConverters.*

/** Prints what dotty 3.9.0 makes of Scala files, in the format of `syntax-dump` (crates/syntax):
 *  tokens, parsed trees, or parsed trees after the desugaring that precedes typing (`Expander`).
 *  Usage: Oracle tokens|trees|desugar --list LIST, one `path<TAB>options` per line.
 */
object Oracle:
  def main(args: Array[String]): Unit =
    val List(mode, "--list", list) = args.toList: @unchecked
    val jobs = Files.readAllLines(Paths.get(list)).asScala.filter(_.nonEmpty).map { l =>
      val parts = l.split('\t')
      (Paths.get(parts(0)), if parts.length > 1 then parts(1).split(' ').toList.filter(_.nonEmpty) else Nil)
    }
    val root = (new ContextBase).initialCtx.fresh
    root.setSetting(root.settings.color, "never")
    if mode != "tokens" then
      root.setSetting(root.settings.Yusejavacp, true)
      root.setReporter(new StoreReporter(null))
      root.initialize()(using root)
    val out = new java.io.PrintStream(new java.io.FileOutputStream(java.io.FileDescriptor.out), false, "UTF-8")
    for (path, opts) <- jobs do
      val b = new StringBuilder(s"== file $path\n")
      try run(mode, root, path, opts, b)
      catch case e: Throwable =>
        val at = e.getStackTrace.take(3).map(f => s"${f.getFileName}:${f.getLineNumber}").mkString(" ")
        b ++= s"== crash ${e.getClass.getName}: ${e.getMessage} @ $at\n"
      out.print(b)
    out.flush()

  def run(mode: String, root: FreshContext, path: Path, opts: List[String], b: StringBuilder): Unit =
    val fctx = root.fresh
    fctx.setSettings(ScalacCommand.distill(opts.toArray, fctx.settings)(fctx.settingsState)(using fctx).sstate)
    fctx.setSetting(fctx.settings.color, "never")
    val reporter = new StoreReporter(null) with UniqueMessagePositions with HideNonSensicalMessages
    fctx.setReporter(reporter)
    given Context = fctx
    val source = SourceFile(AbstractFile.getFile(path.toString), scala.io.Codec.UTF8)
    val uctx = fctx.fresh.setCompilationUnit(CompilationUnit(source, mustExist = true))
    val bytes = utf8Offsets(source.content())
    if mode == "tokens" then tokens(source, bytes, b)(using uctx)
    else
      val parsed = new Parsers.Parser(source)(using uctx).parse()
      val tree =
        if mode == "trees" then parsed
        else
          val ectx = uctx.fresh.setOwner(defn.EmptyPackageClass).setMode(Mode.None).setPhase(uctx.base.typerPhase)
          new Expander().unit(parsed)(using ectx)
      Sexp(bytes, b).tree(tree)(using uctx)
      b += '\n'
    for d <- reporter.removeBufferedMessages(using uctx) do
      val at = if d.pos.span.exists then bytes(d.pos.span.point min (bytes.length - 1)).toString else "-"
      val msg =
        try d.msg.message.replaceAll("\u001b\\[[0-9;]*m", "")
        catch case e: Throwable => s"<unrenderable ${d.msg.getClass.getSimpleName}: ${e.getClass.getSimpleName}>"
      b ++= s"== ${if d.level == interfaces.Diagnostic.ERROR then "E" else "W"} $at ${str(msg)}\n"

  def tokens(source: SourceFile, bytes: Array[Int], b: StringBuilder)(using Context): Unit =
    val s = new Scanner(source)
    def line(): Unit =
      val t = s.token
      val payload =
        if t == IDENTIFIER || t == BACKQUOTED_IDENT || t == INTERPOLATIONID then " " + str(s.name.toString)
        else if t >= CHARLIT && t <= STRINGPART || t == QUOTEID then " " + str(String.valueOf(s.strVal))
        else ""
      val base = if t == INTLIT || t == LONGLIT then s" base=${s.base}" else ""
      val lo = if s.lineOffset < 0 then "-" else bytes(s.lineOffset).toString
      b ++= s"$t ${bytes(s.offset)} ${bytes(s.lastOffset)} $lo$payload$base\n"
    while s.token != EOF do
      line()
      s.nextToken()
    line()

  /** The UTF-8 byte offset of every UTF-16 index, and of the end. */
  def utf8Offsets(cs: Array[Char]): Array[Int] =
    val out = new Array[Int](cs.length + 2)
    var (i, b) = (0, 0)
    while i < cs.length do
      out(i) = b
      val c = cs(i)
      b += (if c < 0x80 then 1 else if c < 0x800 then 2 else if Character.isHighSurrogate(c) then 4
            else if Character.isLowSurrogate(c) && i > 0 && Character.isHighSurrogate(cs(i - 1)) then 0 else 3)
      i += 1
    out(cs.length) = b
    out(cs.length + 1) = b + 1
    out

  def str(s: String): String =
    val b = new StringBuilder("\"")
    s.codePoints.forEach {
      case '"' => b ++= "\\\""
      case '\\' => b ++= "\\\\"
      case '\n' => b ++= "\\n"
      case '\r' => b ++= "\\r"
      case '\t' => b ++= "\\t"
      case c if c < ' ' || c >= 0xD800 && c <= 0xDFFF => b ++= f"\\u$c%04x"
      case c => b.appendAll(Character.toChars(c))
    }
    b += '"'
    b.toString
