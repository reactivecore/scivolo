//> using scala 3.9.0
object Red
object Green

class Node(val id: Int):
  def same(x: Any): String = x match
    case _: this.type => "self"
    case n: Node => "node " + n.id
    case _ => "other"

class V(val n: Int) extends AnyVal:
  def same(x: Any): String = x match
    case _: this.type => "self"
    case _ => "other"

object Q:
  type A = "a"
  type Three = 3
  type OneTwo = 1 | 2
  type R = Red.type
  def alias(x: Any): String = x match
    case _: A => "a"
    case _: Three => "three"
    case _: R => "red"
    case _ => "other"
  def lits(x: Any): String = x match
    case _: OneTwo => "1or2"
    case _ => "other"
  def strs(x: Any): String = x match
    case _: ("a" | "b") => "aorb"
    case _ => "other"
  def colors(x: Any): String = x match
    case _: (Red.type | Green.type) => "rg"
    case _ => "other"
  def nodes(a: Node, b: Node, x: Any): String = x match
    case _: (a.type | b.type) => "aorb"
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
  for x <- List("a", "zzz", 3, 4, Red, Green) do println(Q.alias(x))
  for x <- List(1, 2, 5) do println(Q.lits(x))
  for x <- List("a", "b", "c") do println(Q.strs(x))
  for x <- List(Red, Green, 1) do println(Q.colors(x))
  val m = Node(2)
  for x <- List(n, m, Node(3)) do println(Q.nodes(n, m, x))
  val v = V(1)
  for x <- List(v, V(1), V(2), 1) do println(v.same(x))
