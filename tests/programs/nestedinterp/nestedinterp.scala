//> using scala 3.9.0
extension (sc: StringContext) def j(args: Any*): String = sc.s(args*)

object N:
  def nested(x: Int): String = j"a${s"${x + 1}"}c"
  def formatted(x: Double): String = j"a${f"$x%.2f"}c"
  def raw2(x: Int): String = j"<${raw"\t$x"}>"

@main def nestedinterp(): Unit =
  println(N.nested(1))
  println(N.formatted(1.5))
  println(N.raw2(3))
