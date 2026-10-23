//> using dep com.lihaoyi::fastparse::3.1.1

import fastparse.*, MultiLineWhitespace.*

def ident[$: P]: P[String] = P(CharIn("a-z").rep(1).!)
def num[$: P]: P[Int] = P(CharIn("0-9").rep(1).!.map(_.toInt))
def pair[$: P]: P[(String, Int)] = P(ident ~ "=" ~ num)
def pairs[$: P]: P[Seq[(String, Int)]] = P(pair.rep(sep = ",") ~ End)
def even[$: P]: P[Int] = P(num.filter(_ % 2 == 0))
def at[$: P]: P[(Int, String)] = P(Index ~ ident)
def notX[$: P]: P[Unit] = P(!"x" ~ AnyChar)
def big[$: P]: P[Int] = P(num.flatMap(n => if n > 9 then Pass(n) else Fail))
def opt[$: P]: P[Option[Int]] = P(num.?)

@main def run(): Unit =
  println(parse("a = 1 ,\n bc=22", pairs(using _)))
  println(parse("4", even(using _)).isSuccess)
  println(parse("3", even(using _)).isSuccess)
  println(parse("  abc", at(using _)))
  println(parse("y", notX(using _)).isSuccess)
  println(parse("x", notX(using _)).isSuccess)
  println(parse("12", big(using _)))
  println(parse("", opt(using _)))
  println(parse("a = ", pairs(using _)))
