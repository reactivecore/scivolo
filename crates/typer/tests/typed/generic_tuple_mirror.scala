import scala.deriving.Mirror

object Test:
  val m = summon[Mirror.Of[(
    Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int
      *: Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int *: Int
      *: Int *: Int *: Int *: Int *: Int *: EmptyTuple)]]
  summon[m.MirroredLabel =:= "Tuple"]
