//> using dep com.lihaoyi::pprint::0.9.6

import pprint.PPrinter.BlackWhite

case class Node(name: String, kids: List[Node] = Nil)
enum Color:
  case Red, Green

@main def run(): Unit =
  BlackWhite.pprintln(Node("root", List(Node("a"), Node("b", List(Node("c"))))), width = 20)
  println(BlackWhite.tokenize(Map(1 -> "x")).mkString)
  BlackWhite.pprintln(Color.Green)
  BlackWhite.pprintln((1, "two", 3.0, 'c', None, Some(List(1))))
  BlackWhite.pprintln("multi\nline")
  println(pprint.tprint[Map[String, List[Int]]].plainText)
  println(pprint.tprint[(Int, String) => Boolean].plainText)
  println(pprint.tprint[Option[?]].plainText)
  println(pprint.tprint[(Int, Seq[Char])].plainText)
