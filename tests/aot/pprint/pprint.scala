//> using dep com.lihaoyi::pprint::0.9.6

import pprint.PPrinter.BlackWhite

case class Pt(x: Int, y: Int)
case class Box(name: String, pts: List[Pt], extra: Option[Map[String, Int]])

@main def run(): Unit =
  BlackWhite.pprintln(Pt(1, 2))
  BlackWhite.pprintln(Box("b", List(Pt(1, 2), Pt(3, 4)), Some(Map("k" -> 1))), width = 30)
  println(BlackWhite(Seq(1, 2, 3), showFieldNames = false).plainText)
  println(pprint(Vector("a" -> 1)).plainText)
