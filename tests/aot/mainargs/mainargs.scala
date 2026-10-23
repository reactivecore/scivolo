//> using dep com.lihaoyi::mainargs::0.7.8

import mainargs.{arg, Flag, Leftover, ParserForClass, ParserForMethods}

object LhVar:
  @mainargs.main
  def tags(names: Seq[String], limit: Option[Int], loud: Boolean = false, rest: Leftover[String]): String =
    s"$names $limit $loud ${rest.value}"

  @mainargs.main
  def many(xs: String*): String = xs.mkString("+")

@mainargs.main
case class LhConfig(@arg(short = 'n', doc = "the name") name: String, count: Int = 1, debug: Flag)

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
  println(ParserForClass[LhConfig].constructEither(Seq("-n", "x", "--count", "3")))
  println(ParserForClass[LhConfig].constructEither(Seq("--name", "y", "--debug")))
  println(ParserForClass[LhConfig].constructEither(Seq("--count", "2")))
  val vars = ParserForMethods(LhVar)
  println(vars.runEither(Seq("tags", "--names", "a", "--names", "b", "--limit", "3", "x", "y")))
  println(vars.runEither(Seq("tags", "--loud", "true")))
  println(vars.runEither(Seq("many", "1", "2", "3")))
