class Node(val name: String, val kids: Seq[Int])

object Node {
  def unapplySeq(n: Node): (String, Seq[Int]) = (n.name, n.kids)
}

object SomeNode {
  def unapplySeq(n: Node): Some[(String, Seq[Int])] = Some((n.name, n.kids))
}

object Kids {
  def unapplySeq(n: Node): Seq[Int] = n.kids
}

case class Leaves(name: String, leaves: Int*)

final class Chars(s: String) {
  def length: Int = s.length
  def apply(i: Int): Char = s.charAt(i)
  def apply(i: String): Char = s.charAt(0)
  def drop(n: Int): Seq[Char] = s.toSeq.drop(n)
  def toSeq: Seq[Char] = s.toSeq
}

object Word {
  def unapplySeq(s: String): Chars = new Chars(s)
}

object Main {
  def node(n: Node): Int = n match {
    case Node(name) => name.length
    case Node(name, k, rest*) => k + rest.length
  }
  def some(n: Node): Int = n match {
    case SomeNode(_, ks*) => ks.sum
  }
  def kids(n: Node): Int = n match {
    case Kids(a, b) => a + b
    case _ => 0
  }
  def leaves(l: Leaves): Int = l match {
    case Leaves(n, a, more*) => a + more.length
    case _ => 0
  }
  def word(s: String): Int = s match {
    case Word('a', rest*) => rest.length
    case Word(c, d) => 2
    case _ => 0
  }
}
