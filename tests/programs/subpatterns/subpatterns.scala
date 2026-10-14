//> using scala 3.9.0
case class Point(x: Int, y: Int, label: String)
case class Tree(name: String, kids: Int*)

class Box(val name: String, val items: Seq[Int])
object Box:
  def unapplySeq(b: Box): (String, Seq[Int]) = (b.name, b.items)

object Boxed:
  def unapplySeq(b: Box): Some[(String, Seq[Int])] = Some((b.name, b.items))

object Digits:
  def unapplySeq(n: Int): Seq[Int] =
    if n < 10 then List(n) else if n < 100 then List(n / 10, n % 10) else List(n / 100, n / 10 % 10, n % 10)

final class Chars(s: String):
  def length: Int = s.length
  def apply(i: Int): Char = s.charAt(i)
  def drop(n: Int): Seq[Char] = s.toSeq.drop(n)
  def toSeq: Seq[Char] = s.toSeq

object Word:
  def unapplySeq(s: String): Chars = Chars(s)

object P:
  def typed(x: Any): String = x match
    case Some(i: Int) => s"int $i"
    case Some(s: String) => s"string $s"
    case (a: Int, b: String) => s"pair $a $b"
    case _ => "other"

  def named(p: Point): String = p match
    case Point(label = "origin") => "origin"
    case Point(y = 0, x = x) => s"on x at $x"
    case Point(x = x, label = l) => s"$l at $x"

  def tuple(p: (Any, Any)): String = p match
    case t @ (_, l: List[?]) => s"list of ${l.length} in $t"
    case _ => "no list"

  def boxes(b: Box): String = b match
    case Box(n) => s"$n empty"
    case Box(n, i) => s"$n one $i"
    case Box(n, i, rest*) => s"$n first $i, ${rest.length} more"

  def boxed(b: Box): Int = b match
    case Boxed(_, xs*) => xs.sum

  def tree(t: Tree): String = t match
    case Tree(n) => s"leaf $n"
    case Tree(n, k, ks*) => s"$n: $k then ${ks.mkString(",")}"

  def digits(n: Int): String = n match
    case Digits(d) => s"one digit $d"
    case Digits(a, b) => s"two digits $a $b"
    case Digits(a, _*) => s"starts with $a"

  def word(s: String): String = s match
    case Word('a', rest*) => s"a then ${rest.length}"
    case Word(c, d) => s"two $c$d"
    case _ => "other word"

@main def subpatterns(): Unit =
  println(P.typed(Some(3)))
  println(P.typed(Some("s")))
  println(P.typed((1, "b")))
  println(P.typed(None))
  println(P.named(Point(0, 0, "origin")))
  println(P.named(Point(4, 0, "p")))
  println(P.named(Point(4, 2, "q")))
  val Point(label = l, y = y) = Point(1, 2, "r")
  println(s"$l $y")
  println(P.tuple((1, List(2, 3))))
  println(P.tuple((1, 2)))
  println(P.boxes(Box("a", Nil)))
  println(P.boxes(Box("b", List(7))))
  println(P.boxes(Box("c", List(7, 8, 9))))
  println(P.boxed(Box("d", List(1, 2, 3))))
  println(P.tree(Tree("t")))
  println(P.tree(Tree("u", 1, 2, 3)))
  println(P.digits(7))
  println(P.digits(42))
  println(P.digits(123))
  println(P.word("abc"))
  println(P.word("xy"))
  println(P.word("xyz"))
