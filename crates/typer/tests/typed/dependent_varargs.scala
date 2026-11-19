trait Table:
  type Id = String

object Table1 extends Table:
  val id: Id = "table1_id"

trait Box:
  type T

object IntBox extends Box:
  type T = Int

case class Pair(b: Box, x: b.T)

object Main:
  def kek(t: Table, ids: t.Id*): Int = ids.length
  def keys(t: Table, ids: t.Id*): Seq[t.Id] = ids
  val n = kek(Table1, Table1.id, "b")
  val m = kek(Table1, List("a", "b")*)
  val p = Pair(x = 3, b = IntBox)
  val q = Pair(IntBox, 4)
