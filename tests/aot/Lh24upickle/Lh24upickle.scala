//> using dep com.lihaoyi::upickle::4.4.3

import upickle.default.*
import upickle.implicits.key

case class Pt(x: Int, y: Int) derives ReadWriter
case class Line(name: String, pts: List[Pt], tags: Map[String, Int] = Map.empty, note: Option[String] = None)
    derives ReadWriter

sealed trait Shape derives ReadWriter
case class Circle(r: Double) extends Shape
case class Rect(w: Int, h: Int) extends Shape
case object Empty extends Shape

enum Color derives ReadWriter:
  case Red, Green
  case Mixed(level: Int)

case class Scene(shapes: List[Shape], color: Color) derives ReadWriter

case class Id(value: Int)
object Id:
  given ReadWriter[Id] = readwriter[Int].bimap[Id](_.value, Id(_))

case class User(@key("user_name") name: String, id: Id, tags: Seq[String] = Nil) derives ReadWriter
case class Mix(a: Either[String, Int], b: (Int, String), c: Option[Double]) derives ReadWriter

@main def run(): Unit =
  val l = Line("l", List(Pt(1, 2), Pt(3, 4)), Map("a" -> 1), Some("n"))
  val s = write(l)
  println(s)
  println(read[Line](s) == l)
  println(read[Line]("""{"name":"m","pts":[]}"""))
  println(write(Line("d", Nil)))
  println(read[Map[String, List[Int]]]("""{"a":[1,2]}"""))
  println(write(Pt(5, 6), indent = 2))
  val scene = Scene(List(Circle(1.5), Rect(2, 3), Empty), Color.Mixed(4))
  val js = write(scene)
  println(js)
  println(read[Scene](js) == scene)
  println(write[Shape](Empty))
  println(read[Color]("\"Green\""))
  println(read[Shape]("""{"$type":"Rect","w":1,"h":1}"""))
  val u = User("ann", Id(7), Seq("x"))
  println(write(u))
  println(read[User]("""{"user_name":"bob","id":3}"""))
  println(read[User](writeJs(u)) == u)
  println(write(Mix(Left("e"), (1, "one"), None)))
  println(read[Mix]("""{"a":[1,5],"b":[2,"two"],"c":1.5}"""))
  println(write(Map(1 -> "a")))
