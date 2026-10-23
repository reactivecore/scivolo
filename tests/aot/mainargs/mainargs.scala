//> using dep com.lihaoyi::mainargs::0.7.8

import mainargs.{arg, Flag, ParserForMethods}

object LhCli:
  @mainargs.main(doc = "Repeats a word")
  def repeat(@arg(short = 'w', doc = "the word") word: String, @arg(doc = "how often") times: Int = 2): String =
    word * times

  @mainargs.main
  def add(x: Int, y: Int, @arg(short = 'v') verbose: Flag): String =
    if verbose.value then s"$x + $y = ${x + y}" else (x + y).toString

@main def run(): Unit =
  val parser = ParserForMethods(LhCli)
  println(parser.runEither(Seq("repeat", "--word", "ab", "--times", "3")))
  println(parser.runEither(Seq("repeat", "-w", "x")))
  println(parser.runEither(Seq("add", "-x", "1", "-y", "2")))
  println(parser.runEither(Seq("add", "-v", "-x", "4", "-y", "5")))
  println(parser.runEither(Seq("repeat", "--times", "1")))
  println(parser.runEither(Seq("add", "-x", "1")).isLeft)
  println(parser.runEither(Seq("repeat", "--times", "zz", "-w", "a")).isLeft)
