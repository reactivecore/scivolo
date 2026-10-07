//> using scala 3.9.0
//> using dep org.scala-lang::scala3-repl:3.9.0

// Prints REPL transcripts (`scala> ` input, `     | ` continuation lines, `     |` an empty
// one) as dotty's REPL answers them, one fresh session per file.

import dotty.tools.repl.{ReplDriver, State}
import java.io.{ByteArrayOutputStream, PrintStream}
import java.nio.charset.StandardCharsets.UTF_8

val Prompt = "scala> "
val Continuation = "     |"

def inputs(lines: List[String]): List[List[String]] = lines match
  case Nil => Nil
  case first :: rest if first.startsWith(Prompt) =>
    val (more, after) = rest.span(_.startsWith(Continuation))
    (first :: more) :: inputs(after)
  case _ :: rest => inputs(rest)

def text(line: String): String =
  if line.startsWith(Prompt) then line.stripPrefix(Prompt)
  else line.stripPrefix(Continuation).stripPrefix(" ")

@main def oracle(files: String*): Unit =
  for file <- files do
    println(s"== $file")
    val bytes = ByteArrayOutputStream()
    val out = PrintStream(bytes, true, UTF_8)
    val cp = System.getProperty("java.class.path")
    val driver = ReplDriver(Array("-classpath", cp, "-color:never"), out)
    var state: State = driver.initialState
    for input <- inputs(scala.io.Source.fromFile(file, "UTF-8").getLines().toList) do
      input.foreach(println)
      state = driver.run(input.map(text).mkString("\n"))(using state)
      bytes.toString(UTF_8).linesIterator.map(_.replaceAll("\\s+$", "")).filter(_.nonEmpty).foreach(println)
      bytes.reset()
      println()
