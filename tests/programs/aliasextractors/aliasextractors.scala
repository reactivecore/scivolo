//> using scala 3.9.0
object Trees:
  case class Block[T](stats: List[T], expr: T)
  case class Leaf(value: Int)

object tpd:
  type Block = Trees.Block[Int]
  def Block(stats: List[Int], expr: Int): Block = Trees.Block(stats, expr)
  type Leaf = Trees.Leaf
  def Leaf(value: Int): Leaf = Trees.Leaf(value)

import tpd.*

def describe(t: Any): String = t match
  case Block(stats, expr) => s"block ${stats.length} $expr"
  case Leaf(v) => s"leaf $v"
  case _ => "other"

@main def aliasextractors(): Unit =
  println(describe(Block(List(1, 2), 3)))
  println(describe(Leaf(7)))
  println(describe("x"))
