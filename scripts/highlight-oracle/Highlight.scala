//> using scala 3.9.0
//> using dep org.scala-lang::scala3-repl:3.9.0

// Prints dotty's `SyntaxHighlighting.highlight` of each line of the given files (`\n` in a
// line stands for a newline), with ESC shown as `\e`.

import dotty.tools.dotc.core.Contexts.*
import dotty.tools.dotc.printing.SyntaxHighlighting

@main def highlight(files: String*): Unit =
  val base = new ContextBase
  val ctx0 = base.initialCtx.fresh
  ctx0.setSetting(ctx0.settings.classpath, System.getProperty("java.class.path"))
  ctx0.setSetting(ctx0.settings.color, "always")
  ctx0.base.initialize()(using ctx0)
  given Context = ctx0
  for file <- files do
    for line <- scala.io.Source.fromFile(file, "UTF-8").getLines() do
      val in = line.replace("\\n", "\n")
      val out = SyntaxHighlighting.highlight(in)
      println(out.replace("\u001b", "\\e").replace("\n", "\\n"))
