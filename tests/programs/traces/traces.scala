//> using scala 3.9.0

class Parser:
  def parse(s: String): Int = digits(s)
  @noinline private def digits(s: String): Int = s.toInt

class Checker:
  def check(i: Int): Int = if i < 0 then fail(i) else i
  def fail(i: Int): Int = throw IllegalArgumentException(s"negative $i")

def run(c: Checker, i: Int): Int = c.check(i)

def both(c: Checker, i: Int, j: Int): Int =
  val a = c.check(i)
  a + c.check(j)

def show(e: Throwable): Unit =
  val frames = e.getStackTrace.toList
  val ours = frames.filter(f => Seq("Parser", "Checker", "traces").exists(f.getClassName.startsWith))
  println(ours.map(f => s"${f.getClassName}.${f.getMethodName} ${f.getFileName}:${f.getLineNumber}").mkString("\n"))

/** Stack traces name the interpreted frames with file and line, also for methods inlined into
  * others once hot; `traces.check` is written by hand.
  */
@main def traces(): Unit =
  try new Parser().parse("x1")
  catch case e: NumberFormatException => show(e)
  val c = Checker()
  var sum = 0
  for i <- 0 until 1500 do sum += run(c, i)
  try run(c, -1)
  catch case e: IllegalArgumentException => show(e)
  for i <- 0 until 1500 do sum += both(c, i, i)
  try both(c, 1, -2)
  catch case e: IllegalArgumentException => show(e)
