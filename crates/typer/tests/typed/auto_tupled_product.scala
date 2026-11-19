case class E(p: (Int, String))

object Test:
  val a = E((1, "a")) match
    case E(n, s) => s * n
