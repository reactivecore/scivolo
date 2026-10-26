case class Pet(kind: String, age: Int)

class Row(values: Map[String, Any]) extends Selectable:
  type Fields = NamedTuple.From[Pet]
  def selectDynamic(field: String): Any = values(field)

object Ta26NamedTupleTrees:
  type Person = (name: String, age: Int)
  def name(p: Person): String = p.name
  def kind(r: Row): String = r.kind
