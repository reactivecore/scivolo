class Kind {
  def isKind: Boolean = true
}

class Node[-T >: Null] {
  def tpe: T @annotation.unchecked.uncheckedVariance = ???
}

case class Sel[-T >: Null](qual: Node[T]) extends Node[T]

object GadtUpcastBound {
  def test[T <: Node[Kind]](node: T) = node match {
    case Sel(q) => q.tpe.isKind
  }
}
