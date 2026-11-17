package scala.probe

class Views {
  def toLong(i: Int): Long = i
  def toDouble(c: Char, f: Float): Double = c + f
  def lit(i: Int): Long = i + 1
  def scalaInt(i: scala.Int): Long = i
  def str(s: String): Seq[Char] = s
}
