//> using scala 3.9.0
object Table:
  private val DefaultSize = 4
  private val Primes: Array[Int] = Array(2, 3, 5, 7)

class Table(val size: Int):
  import Table.*
  def this() = this(Table.DefaultSize)
  def sum: Int = Primes(0) + DefaultSize + size

class Node(val hash: Int)

class TreeNode(hash: Int, val left: TreeNode | Null) extends Node(hash * 2):
  def leftHash: Int = if left == null then 0 else left.nn.hash

class Meters(val value: Int):
  protected def >(o: Int): Boolean = value > o

object Meters:
  given Conversion[Meters, Int] = _.value

@main def companionprivates(): Unit =
  println(new Table().sum + Table(1).sum)
  println(TreeNode(3, TreeNode(5, null)).leftHash)
  println(Meters(7) > 5)
