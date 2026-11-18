class Tree

sealed trait TreeParts:
  this: Tree.type =>
  def me: Tree.type = this

object Tree extends TreeParts
