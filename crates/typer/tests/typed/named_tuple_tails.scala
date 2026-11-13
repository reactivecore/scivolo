import scala.NamedTuple.*

trait Box:
  type E

object NamedTupleTails:
  type P = (name: String, age: Int, ok: Boolean)
  val p: P = ("a", 1, true)
  def t = p.tail.tail
  def o = Option(p.tail)
  def e: DropNames[P] = p.toTuple

  def ov(b: Box): b.E = ???
  def ov(b: Box, s: String): Int = 1
  val bx: Box { type E = Int } = ???
  def u = ov(bx)
