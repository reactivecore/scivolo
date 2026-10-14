//> using scala 3.9.0
object Red
object Green

class Node(val id: Int):
  def same(x: Any): String = x match
    case _: this.type => "self"
    case n: Node => "node " + n.id
    case _ => "other"

object P:
  type IS = Int | String
  def size(x: Any): Int = x match
    case xs: List[?] => xs.size
    case m: Map[_, _] => m.size + 100
    case o: Option[_] => if o.isEmpty then -1 else -2
    case _ => 0
  def color(x: Any): String = x match
    case _: Red.type => "red"
    case _: Green.type => "green"
    case _ => "none"
  def lit(x: Any): Int = x match
    case s: "a" => s.length
    case _: "bc" => 2
    case _: 3 => 30
    case _: 'c' => 40
    case _: true => 50
    case _ => 0
  def is(a: AnyRef, x: Any): Boolean = x match
    case _: a.type => true
    case _ => false
  def union(x: Any): String = x match
    case v: IS => "is " + v
    case _ => "neither"

@main def typepatterns2(): Unit =
  println(P.size(List(1, 2, 3)))
  println(P.size(Map(1 -> 2)))
  println(P.size(Some(1)))
  println(P.size(None))
  println(P.size("x"))
  println(P.color(Red))
  println(P.color(Green))
  println(P.color(1))
  println(P.lit("a"))
  println(P.lit("bc"))
  println(P.lit(3))
  println(P.lit(3L))
  println(P.lit('c'))
  println(P.lit(true))
  println(P.lit("z"))
  val n = Node(1)
  println(n.same(n))
  println(n.same(Node(2)))
  println(n.same(3))
  println(P.is(n, n))
  println(P.is(n, Node(1)))
  println(P.union(4))
  println(P.union("s"))
  println(P.union(2.5))
