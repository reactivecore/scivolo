trait Tag
type Tags = List[Tag]
type Same = Tags

class Holder { def tags: Tags = Nil }

object LubOfTermReferences {
  def aliasLast(c: Boolean, h: Holder) = if c then h.tags else (Nil: Tags)
  def aliasFirst(c: Boolean, h: Holder) = if c then (Nil: Tags) else h.tags
  def otherAlias(c: Boolean, h: Holder) = if c then (Nil: Same) else h.tags
  def targetFirst(c: Boolean, h: Holder) = if c then (Nil: List[Tag]) else h.tags
  def targetLast(c: Boolean, h: Holder) = if c then h.tags else (Nil: List[Tag])
  def inMatch(i: Int, h: Holder) = i match {
    case 0 => h.tags
    case _ => Nil: List[Tag]
  }
}
