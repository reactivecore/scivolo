package lib

import scala.NamedTuple.{AnyNamedTuple, NamedTuple}

object Decomposition:
  type Names[X <: AnyNamedTuple] <: Tuple = X match
    case NamedTuple[n, ?] => n
  type DropNames[X <: AnyNamedTuple] <: Tuple = X match
    case NamedTuple[?, x] => x

object Forwarding:
  export Decomposition.{Names, DropNames}
  type Mapped[X <: AnyNamedTuple, F[_ <: Tuple.Union[DropNames[X]]]] =
    NamedTuple[Names[X], Tuple.Map[DropNames[X], F]]
