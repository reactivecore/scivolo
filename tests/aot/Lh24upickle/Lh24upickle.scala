//> using dep com.lihaoyi::upickle::4.4.3

import upickle.default.{ReadWriter, read, write}

case class Pt(x: Int, y: Int) derives ReadWriter
case class Line(name: String, pts: List[Pt], tags: Map[String, Int] = Map.empty, note: Option[String] = None)
    derives ReadWriter

@main def run(): Unit =
  val l = Line("l", List(Pt(1, 2), Pt(3, 4)), Map("a" -> 1), Some("n"))
  val s = write(l)
  println(s)
  println(read[Line](s) == l)
  println(read[Line]("""{"name":"m","pts":[]}"""))
  println(write(Line("d", Nil)))
  println(read[Map[String, List[Int]]]("""{"a":[1,2]}"""))
  println(write(Pt(5, 6), indent = 2))
