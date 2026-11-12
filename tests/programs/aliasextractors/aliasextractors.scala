//> using scala 3.9.0
object Trees:
  case class Block[T](stats: List[T], expr: T)
  case class Leaf(value: Int)
  case class New[T](tpt: T)

object tpd:
  type Block = Trees.Block[Int]
  def Block(stats: List[Int], expr: Int): Block = Trees.Block(stats, expr)
  type Leaf = Trees.Leaf
  def Leaf(value: Int): Leaf = Trees.Leaf(value)
  type New = Trees.New[Int]
  def New(tpt: Int)(using tag: String): New = Trees.New(tpt)
  def New(tpt: String)(using tag: String): New = Trees.New(tpt.length)

trait Aliases:
  type Block = Trees.Block[Int]

class Inherited extends Aliases:
  def last(t: Any): Any = t match
    case Block(_, expr) => expr
    case _ => "none"

import tpd.*

def describe(t: Any): String = t match
  case Block(stats, expr) => s"block ${stats.length} $expr"
  case Leaf(v) => s"leaf $v"
  case New(tpt: Int) => s"new $tpt"
  case _ => "other"

def size(t: Any): Int = t match
  case tpd.New(tpt: Int) => tpt
  case _ => 0

@main def aliasextractors(): Unit =
  given String = "tag"
  println(describe(Block(List(1, 2), 3)))
  println(describe(Leaf(7)))
  println(describe(New("abcd")))
  println(describe("x"))
  println(size(New(5)))
  println(Inherited().last(Block(Nil, 9)))
