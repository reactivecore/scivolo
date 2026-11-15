class SB
trait Ops {
  def addString(b: SB, start: String, sep: String, end: String): b.type = b
  final def addString(b: SB, sep: String): b.type = addString(b, "", sep, "")
  final def addString(b: SB): b.type = addString(b, "")
  def widened(b: SB): SB = addString(b, "")
  def other(b: SB, c: SB): c.type = addString(c, "", "", "")
  def tied(b: SB, i: Int): b.type = b
  def tied(b: SC, i: Any): String = ""
  def deep(c: SC): c.type = tied(c, 1)
}
class SC extends SB
