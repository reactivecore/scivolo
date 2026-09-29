//> using scala 3.9.0
//> using dep org.scala-lang::scala3-compiler:3.9.0

package dotty.tools.dotc.syntaxoracle

import dotty.tools.dotc.*
import dotty.tools.dotc.ast.{Trees, untpd}
import dotty.tools.dotc.core.Contexts.*
import dotty.tools.dotc.core.{Constants, Names}
import dotty.tools.dotc.parsing.Parsers
import dotty.tools.dotc.parsing.Scanners.Scanner
import dotty.tools.dotc.parsing.Tokens.*
import dotty.tools.dotc.reporting.{Diagnostic, HideNonSensicalMessages, StoreReporter, UniqueMessagePositions}
import dotty.tools.dotc.util.SourceFile
import dotty.tools.dotc.util.Spans.Span
import dotty.tools.io.AbstractFile
import java.nio.file.{Files, Path, Paths}
import scala.jdk.CollectionConverters.*

/** Prints what dotty 3.9.0 makes of Scala files, in the format of `syntax-dump` (crates/syntax).
 *  Usage: Oracle tokens|trees --list LIST, one `path<TAB>options` per line.
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
    if mode == "trees" then
      root.setSetting(root.settings.Yusejavacp, true)
      root.setReporter(new StoreReporter(null))
      root.initialize()(using root)
    val out = new java.io.PrintStream(new java.io.FileOutputStream(java.io.FileDescriptor.out), false, "UTF-8")
    for (path, opts) <- jobs do
      val b = new StringBuilder(s"== file $path\n")
      try run(mode, root, path, opts, b)
      catch case e: Throwable => b ++= s"== crash ${e.getClass.getName}: ${e.getMessage}\n"
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
    if mode == "trees" then
      val tree = new Parsers.Parser(source)(using uctx).parse()
      Sexp(bytes, b).tree(tree)(using uctx)
      b += '\n'
    else tokens(source, bytes, b)(using uctx)
    for d <- reporter.removeBufferedMessages(using uctx) do
      val at = if d.pos.span.exists then bytes(d.pos.span.point).toString else "-"
      val msg = d.msg.message.replaceAll("\u001b\\[[0-9;]*m", "")
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

/** Untyped trees as S-expressions: `(Class span [@backquoted] fields)`, offsets in UTF-8 bytes. */
class Sexp(bytes: Array[Int], b: StringBuilder):
  import untpd.*

  def at(o: Int): Int = bytes(o min (bytes.length - 1))

  def span(s: Span): String =
    if !s.exists then "-"
    else if s.isSynthetic then s"${at(s.start)}..${at(s.end)}"
    else s"${at(s.start)}..${at(s.end)}@${at(s.point)}"

  def name(n: Names.Name): String = (if n.isTypeName then "T:" else "t:") + Oracle.str(n.toString).drop(1).dropRight(1)

  def const(c: Constants.Constant): String =
    val tag = c.tag match
      case Constants.UnitTag => "Unit"; case Constants.BooleanTag => "Boolean"; case Constants.ByteTag => "Byte"
      case Constants.ShortTag => "Short"; case Constants.CharTag => "Char"; case Constants.IntTag => "Int"
      case Constants.LongTag => "Long"; case Constants.FloatTag => "Float"; case Constants.DoubleTag => "Double"
      case Constants.StringTag => "String"; case Constants.NullTag => "Null"; case Constants.ClazzTag => "Class"
      case t => s"tag$t"
    if c.tag == Constants.UnitTag || c.tag == Constants.NullTag then s"#$tag" else s"#$tag:${Oracle.str(c.stringValue)}"

  def mods(m: Modifiers)(using Context): Unit =
    b ++= "(Mods ["
    b ++= m.flags.flagsString.split(' ').filter(_.nonEmpty).mkString(" ")
    b += ']'
    if !m.privateWithin.isEmpty then b ++= " within=" ++= name(m.privateWithin)
    b ++= " ["; m.annotations.foreach { a => b += ' '; tree(a) }; b ++= "]"
    b ++= " ["; m.mods.foreach { md => b ++= s" (${md.getClass.getSimpleName} ${span(md.span)})" }; b ++= "]"
    b += ')'

  def value(v: Any)(using Context): Unit = v match
    case t: Tree => tree(t)
    case m: Modifiers => mods(m)
    case n: Names.Name => b ++= name(n)
    case c: Constants.Constant => b ++= const(c)
    case l: List[?] => b += '['; l.foreach { x => b += ' '; value(x) }; b ++= "]"
    case s: String => b ++= Oracle.str(s)
    case null => b ++= "null"
    case e: (Boolean | Int) => b ++= e.toString
    case e: scala.reflect.Enum => b ++= e.toString
    case other => b ++= s"<${other.getClass.getSimpleName}>"

  def tree(t: Tree)(using Context): Unit =
    if t eq EmptyTree then { b ++= "_"; return }
    if t eq EmptyValDef then { b ++= "_self"; return }
    if t eq EmptyTypeIdent then { b ++= "_tident"; return }
    val cls = Some(t.getClass.getSimpleName).filter(_.nonEmpty).getOrElse(t.productPrefix)
    b += '(' ++= cls += ' ' ++= span(t.span)
    if t.hasAttachment(Trees.Backquoted) then b ++= " @backquoted"
    if t.hasAttachment(Trees.CaptureVar) then b ++= " @capturevar"
    if t.hasAttachment(untpd.RetainsAnnot) then b ++= " @retains"
    t match
      case t: MemberDef => b += ' '; mods(t.rawMods)
      case t: FunctionWithMods => b += ' '; mods(t.mods); b ++= " erased=" ++= t.erasedParams.mkString("[", " ", "]")
      case t: DerivingTemplate => b ++= s" derived=${t.derived.length}"
      case _ =>
    t match
      case t: DefDef => b += ' '; value(t.name); b += ' '; value(t.paramss); b += ' '; tree(t.tpt); b += ' '; tree(t.rhs)
      case t: ValDef => b += ' '; value(t.name); b += ' '; tree(t.tpt); b += ' '; tree(t.rhs)
      case t: Template => b += ' '; tree(t.constr); b += ' '; value(t.parentsOrDerived); b += ' '; tree(t.self); b += ' '; value(t.body)
      case t: TypedSplice => b ++= " <typed>"
      case t: Product => t.productIterator.foreach { f => b += ' '; value(f) }
    b += ')'
