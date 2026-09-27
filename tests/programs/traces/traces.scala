//> using scala 3.9.0

class Parser:
  def parse(s: String): Int = digits(s)
  @noinline private def digits(s: String): Int = s.toInt

/** Stack traces name the interpreted frames with file and line; `traces.check` is written by
  * hand.
  */
@main def traces(): Unit =
  try new Parser().parse("x1")
  catch
    case e: NumberFormatException =>
      val frames = e.getStackTrace.toList
      val names = frames.map(f => s"${f.getClassName}.${f.getMethodName}")
      println(names.filter(n => n.startsWith("Parser") || n.startsWith("traces")).mkString("\n"))
      val parse = frames.find(f => f.getClassName == "Parser" && f.getMethodName == "parse")
      println(parse.map(f => s"${f.getFileName}:${f.getLineNumber}").getOrElse("no parse frame"))
