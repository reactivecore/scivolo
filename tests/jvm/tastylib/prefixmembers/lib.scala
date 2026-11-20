package trees

abstract class Tree[+T <: AnyRef]:
  type ThisTree[T <: AnyRef] <: Tree[T]
  def withType(tpe: String): ThisTree[String] = this.asInstanceOf[ThisTree[String]]

case class Hole[+T <: AnyRef](idx: Int) extends Tree[T]:
  type ThisTree[+T <: AnyRef] <: Hole[T]
