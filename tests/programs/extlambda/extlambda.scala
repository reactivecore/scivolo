final class Ops(private val s: String) extends AnyVal {
  private def escape(ch: Char): String = ch.toString
  def split(separators: Array[Char]): String = separators.foldLeft("[")(_ + escape(_)) + "]"
}

object extlambda {
  def main(args: Array[String]): Unit = println(new Ops("x").split(Array('a', 'b')))
}
