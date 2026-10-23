//> using dep com.lihaoyi::fastparse::3.1.1

import fastparse.*, NoWhitespace.*

def number[$: P]: P[Int] = P(CharIn("0-9").rep(1).!.map(_.toInt))
def parens[$: P]: P[Int] = P("(" ~/ addSub ~ ")")
def factor[$: P]: P[Int] = P(number | parens)
def divMul[$: P]: P[Int] = P(factor ~ (CharIn("*/").! ~/ factor).rep).map(eval)
def addSub[$: P]: P[Int] = P(divMul ~ (CharIn("+\\-").! ~/ divMul).rep).map(eval)
def expr[$: P]: P[Int] = P(addSub ~ End)
def words[$: P]: P[Seq[String]] = P("[" ~ CharsWhileIn("a-z").!.rep(sep = ",") ~ "]" ~ End)
def keyword[$: P]: P[String] = P(StringIn("let", "var").! ~ "!".?)
def sum[$: P]: P[Int] = P(CharsWhileIn("0-9").!.map(_.toInt).rep(min = 1, sep = "+")).map(_.sum)

def eval(tree: (Int, Seq[(String, Int)])): Int =
  val (base, ops) = tree
  ops.foldLeft(base) {
    case (left, ("+", right)) => left + right
    case (left, ("-", right)) => left - right
    case (left, ("*", right)) => left * right
    case (left, (_, right))   => left / right
  }

@main def run(): Unit =
  println(parse("1+2*3", expr(using _)))
  println(parse("(1+2)*3-4/2", expr(using _)))
  println(parse("1+*2", expr(using _)))
  println(parse("[ab,c,def]", words(using _)))
  println(parse("let!", keyword(using _)))
  println(parse("val", keyword(using _)).isSuccess)
  println(parse("1+22+333", sum(using _)))
  val Parsed.Success(v, _) = parse("2*(3+4)", expr(using _)): @unchecked
  println(v)
