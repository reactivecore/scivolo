class Trees[T] {
  class Tree(val v: T)
  type Ref = Tree
}

trait TreeInfo[T] { self: Trees[T] =>
  def isIdem(tree: Tree): Boolean = tree.v != null
  def first(trees: List[Ref]): Ref = trees.head
  def same(t: Tree) = t
}

object tpd extends Trees[String] with TreeInfo[String]

object Use {
  def f(t: tpd.Tree): Boolean = tpd.isIdem(t)
  def g(t: tpd.Tree) = tpd.first(List(t))
  def h(t: tpd.Tree) = tpd.same(t)
}
