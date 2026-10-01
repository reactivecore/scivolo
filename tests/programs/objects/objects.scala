//> using scala 3.9.0
object Cfg:
  val name = "cfg"
  val size = 3
  def greet(who: String): String = who

@main def objects(): Unit =
  println(Cfg.name)
  println(Cfg.size)
  println(Cfg.greet(Cfg.name))
