trait Leaf
case class Elem[A <: Leaf](a: A) extends Leaf
case object Empty extends Leaf

object GadtUnionOperand {
  type Doc = Leaf | Unit | ((Int | String) & Singleton)

  def size(d: Doc): Int = d match {
    case Elem(a) => 1
    case () => 0
    case _ => 2
  }
}
