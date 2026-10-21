//> using scala 3.9.0
//> using dep org.scala-lang::scala3-compiler:3.9.0

package dotty.tools.dotc.typeroracle

import dotty.tools.dotc.*
import dotty.tools.dotc.ast.tpd
import dotty.tools.dotc.core.Contexts.*
import dotty.tools.dotc.core.Phases
import dotty.tools.dotc.reporting.{Diagnostic, HideNonSensicalMessages, StoreReporter, UniqueMessagePositions}
import dotty.tools.io.AbstractFile
import java.nio.file.{Files, Paths}
import scala.jdk.CollectionConverters.*

/** Prints what dotty 3.9.0's typer makes of Scala files, in the format of `scivolo-typer-dump`
 *  (crates/typer): per file its typed tree, its diagnostics and its feature counts.
 *  Usage: Oracle [--inlined] --list LIST, one `path[;path...]<TAB>options` per line, the paths of a
 *  line typed in one run; `--fresh` among the options types them with a compiler of their own.
 *  With `--inlined`, compilation stops after the `inlining` phase and the tree line holds only the
 *  outermost `Inlined` trees, in order.
 */
object Oracle:
  var inlined = false

  def main(args: Array[String]): Unit =
    inlined = args.contains("--inlined")
    val List("--list", list) = args.toList.filter(_ != "--inlined"): @unchecked
    val jobs = Files.readAllLines(Paths.get(list)).asScala.filter(_.nonEmpty).map { l =>
      val parts = l.split('\t')
      (parts(0).split(';').toList, if parts.length > 1 then parts(1).split(' ').toList.filter(_.nonEmpty) else Nil)
    }
    var base = newBase()
    val out = new java.io.PrintStream(new java.io.FileOutputStream(java.io.FileDescriptor.out), false, "UTF-8")
    for (paths, opts0) <- jobs do
      val b = new StringBuilder
      val fresh = opts0.contains("--fresh")
      val opts = opts0.filter(_ != "--fresh")
      val (compiler, root) = if fresh then newBase(opts) else base
      try run(compiler, root, paths, opts, b)
      catch case e: Throwable =>
        val at = e.getStackTrace.take(3).map(f => s"${f.getFileName}:${f.getLineNumber}").mkString(" ")
        b.clear()
        for p <- paths do b ++= s"== file $p\n== crash ${e.getClass.getName}: ${e.getMessage} @ $at\n"
        if !fresh then base = newBase()
      out.print(b)
      out.flush()

  /** A compiler with the standard library, and the `-classpath` of `opts`, on its class path. */
  def newBase(opts: List[String] = Nil): (Compiler, FreshContext) =
    val root = (new ContextBase).initialCtx.fresh
    root.setSetting(root.settings.color, "never")
    root.setSetting(root.settings.Yusejavacp, true)
    opts.sliding(2).collect { case List("-classpath", cp) => cp }.foreach(cp => root.setSetting(root.settings.classpath, cp))
    root.setReporter(new StoreReporter(null))
    root.initialize()(using root)
    (new Compiler, root)

  /** Removes the symbols `run` defined from their package scopes, so that later runs do not see them. */
  def cleanup(run: Run)(using Context): Unit =
    def drop(t: tpd.Tree): Unit = t match
      case tpd.PackageDef(pid, stats) =>
        val pkg = pid.symbol.moduleClass.asClass
        for s <- stats do s match
          case d: tpd.MemberDef if d.symbol.exists && d.symbol.owner == pkg => pkg.delete(d.symbol)
          case d: tpd.PackageDef => drop(d)
          case _ =>
      case _ =>
    for unit <- run.units do drop(unit.tpdTree)

  def run(compiler: Compiler, root: FreshContext, paths: List[String], opts: List[String], b: StringBuilder): Unit =
    val fctx = root.fresh
    fctx.setSettings(ScalacCommand.distill(opts.toArray, fctx.settings)(fctx.settingsState)(using fctx).sstate)
    fctx.setSetting(fctx.settings.color, "never")
    fctx.setSetting(fctx.settings.YstopAfter, List(if inlined then "inlining" else "typer"))
    val reporter = new StoreReporter(null) with UniqueMessagePositions with HideNonSensicalMessages
    fctx.setReporter(reporter)
    val run = compiler.newRun(using fctx)
    run.compile(paths.map(p => AbstractFile.getFile(p)))
    given Context = run.runContext.withPhase(
      if inlined then Phases.inliningPhase(using run.runContext) else Phases.typerPhase(using run.runContext))
    val ds = reporter.removeBufferedMessages
    val units = run.units.filter(_.tpdTree ne tpd.EmptyTree).map(u => u.source.path -> u).toMap
    val elsewhere = ds.filterNot(d => d.pos.span.exists && units.contains(d.pos.source.path))
    for (p, i) <- paths.zipWithIndex do
      b ++= s"== file $p\n"
      val path = AbstractFile.getFile(p).path
      for unit <- units.get(path) do
        val bytes = utf8Offsets(unit.source.content())
        if inlined then
          b += '['
          for t <- outermostInlined(unit.tpdTree) do { b += ' '; Sexp(bytes, b).tree(t) }
          b += ']'
        else Sexp(bytes, b).tree(unit.tpdTree)
        b += '\n'
        for d <- ds if d.pos.span.exists && d.pos.source.path == path do diag(d, bytes(d.pos.span.point min (bytes.length - 1)).toString, b)
        val stats = Stats()
        stats.traverse(unit.tpdTree)
        b ++= "== stats " ++= stats.counts.toSeq.sorted.map((k, v) => s"$k=$v").mkString(" ") += '\n'
      if i == 0 then for d <- elsewhere do diag(d, "-", b)
    cleanup(run)

  /** The `Inlined` trees of `t` not inside another one, in pre-order. */
  def outermostInlined(t: tpd.Tree)(using Context): List[tpd.Inlined] =
    val out = List.newBuilder[tpd.Inlined]
    new tpd.TreeTraverser:
      def traverse(t: tpd.Tree)(using Context): Unit = t match
        case t: tpd.Inlined => out += t
        case _ => traverseChildren(t)
    .traverse(t)
    out.result()

  def diag(d: Diagnostic, at: String, b: StringBuilder): Unit =
    val msg =
      try d.msg.message.replaceAll("\u001b\\[[0-9;]*m", "")
      catch case e: Throwable => s"<unrenderable ${d.msg.getClass.getSimpleName}: ${e.getClass.getSimpleName}>"
    b ++= s"== ${if d.level == interfaces.Diagnostic.ERROR then "E" else "W"} $at ${str(msg)}\n"

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
