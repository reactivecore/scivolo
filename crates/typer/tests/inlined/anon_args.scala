trait Logger:
  def log(a: String): Unit

object Messages:
  def logMessage(a: String): Unit = println(a)

object Test:
  inline def visit[T](inline x: T)(inline f: T => Any): Any = f(x)
  def main(args: Array[String]): Unit =
    visit(new Logger { def log(a: String) = println(a) })(_.log("a"))
    visit(new Logger { export Messages.{logMessage => log} })(_.log("b"))
