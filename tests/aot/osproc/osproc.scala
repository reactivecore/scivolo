//> using dep com.lihaoyi::os-lib::0.11.9-M8

@main def run(): Unit =
  val echo = os.proc("echo", "hello", "proc").call()
  println(echo.exitCode)
  println(echo.out.text().trim)
  println(os.proc("sh", "-c", "exit 3").call(check = false).exitCode)
  val lines = os.proc("printf", "a\\nb\\nc\\n").call().out.lines()
  println(lines.size -> lines.last)
