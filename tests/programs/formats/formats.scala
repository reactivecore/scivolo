//> using scala 3.9.0
class Point(val x: Int, val y: Int):
  override def toString: String = f"($x%d, $y%d)"

object F:
  def numbers(i: Int, l: Long, s: Short, b: Byte): String =
    f"$i%5d|$i%-5d|$i%05d|$i%x|$i%X|$i%o|$l%,d|$s%d|$b%+d"
  def floats(d: Double, f: Float): String =
    f"$d%.2f|$d%10.4f|$d%e|$d%.3E|$d%g|$f%.1f|${d * 2}%08.3f"
  def others(c: Char, z: Boolean, s: String, p: Point): String =
    f"$c%c|${'A'.toInt}%c|$z%b|$s%s|$s%S|$s%10s|$s%-4s|$p|${p.x}"
  def literals(n: Int): String = f"100%% of $n\t%n${n + 1}%d%%"
  def indexed(a: String, b: Int): String = f"$a%s-$b%d-${a}%1$$s-$b%<d"
  def big(b: BigInt, d: BigDecimal): String = f"$b%d|$b%x|$d%.3f"
  def plain(): String = f"no arguments %%"
  def nested(n: Int): String = f"[${f"$n%03d"}%s]"
  def mixed(n: Int): String = s"<${f"$n%2d"}>" + f"${s"n=$n"}%5s"
  def mapped(xs: List[Int]): String = xs.map(i => f"$i%02d").mkString(",")

@main def formats(): Unit =
  println(F.numbers(255, 1234567L, 7, -3))
  println(F.floats(3.14159, 2.5f))
  println(F.others('x', true, "abc", Point(1, 2)))
  println(F.literals(9))
  println(F.indexed("q", 4))
  println(F.big(BigInt(255), BigDecimal("1.23456")))
  println(F.plain())
  println(F.nested(7))
  println(F.mixed(3))
  println(F.mapped(List(1, 22, 3)))
