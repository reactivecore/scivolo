import scala.language.experimental.modularity
import scala.language.future

trait Coll[Self]:
  type Pos
  def first(s: Self): Pos

given Coll[String]:
  type Pos = Char
  def first(s: String): Char = s.head

given listColl: [S] => (c: Coll[S]) => Coll[List[S]]:
  type Pos = c.Pos
  def first(s: List[S]): c.Pos = c.first(s.head)

given optColl: [S: Coll as c] => Coll[Option[S]]:
  type Pos = c.Pos
  def first(s: Option[S]): c.Pos = c.first(s.get)

val l = summon[Coll[List[String]]]

def head[S: Coll as c](s: S): c.Pos = c.first(s)

class Holder[S: Coll as c](val s: S):
  def pos: c.Pos = c.first(s)

class Named(val coll: Coll[String])

val h = Holder("ab")
val p: Char = h.pos
val q: Char = head("ab")
val n = Named(summon[Coll[String]])
val r: Char = n.coll.first("x")
