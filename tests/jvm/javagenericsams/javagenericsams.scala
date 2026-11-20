import java.util.Optional

@main def javagenericsams =
  val length = Optional.of("abc").map(_.length).orElse(0)
  val text = Optional.of("abc").map(s => s + "d").orElse("e")
  val none = Optional.empty[String]().map(_.length).orElse(-1)
  println((length, text, none))
