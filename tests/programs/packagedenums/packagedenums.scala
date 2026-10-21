package shapes {
  enum Color:
    case Red, Green
}

@main def packagedenums(): Unit =
  import scala.util.Try
  println(shapes.Color.valueOf("Red"))
  println(shapes.Color.fromOrdinal(1))
  println(Try(shapes.Color.valueOf("Blue")))
  println(Try(shapes.Color.fromOrdinal(2)))
