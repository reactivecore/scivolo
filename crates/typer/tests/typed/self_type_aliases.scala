object Trees {
  class Tree[T](val v: T)
  abstract class Instance[T] {
    type Tree = Trees.Tree[T]
    def mk(v: T): Tree = new Trees.Tree(v)
  }
}

trait TreeInfo[T] { self: Trees.Instance[T] =>
  def unsplice(tree: Trees.Tree[T]): Trees.Tree[T] = tree
  def isDecl(tree: Tree): Boolean = unsplice(tree) match {
    case t: Trees.Tree[?] => true
  }
}

object tpd extends Trees.Instance[String] with TreeInfo[String]
