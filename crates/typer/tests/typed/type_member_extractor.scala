trait Column:
  type T
  type F[X]
  type Q = F[T]

class varchar extends Column:
  type T = String

trait notnull extends Column:
  type F[X] = X

object Objects:
  object firstName extends varchar with notnull
  object lastName extends varchar with notnull
  val relation = (firstName, lastName)
  type RelationTypes = Tuple.InverseMap[relation.type, [X] =>> Column { type Q = X }]
  summon[RelationTypes =:= (String, String)]
