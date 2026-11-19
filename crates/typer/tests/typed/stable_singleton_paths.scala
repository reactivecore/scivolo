abstract class AbsCell:
  type T = Node
  val init: T
  class Node

class Super:
  final val q = ""
  def q1: q.type = q

class Sub extends Super:
  def same: Option[Sub.super.q.type] = Some[Sub.super.q.type](q1)

object Main:
  val x = new Object
  class A(var y: x.type)
  val a = new A(x)
  val y: a.y.type = x
  def f(): Unit =
    val cell = new AbsCell { val init = new Node() }
    val node = new cell.type#T()
