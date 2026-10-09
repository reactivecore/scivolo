//> using scala 3.9.0
import scala.annotation.tailrec

final class Chain(val v: Int, val rest: Chain):
  def last(n: Int): Int = if rest == null || n == 0 then v else rest.last(n - 1)

object T:
  def fact(n: Int, acc: Long): Long = if n == 0 then acc else fact(n - 1, acc * n)
  def count(n: Int, k: Int): Int = if n == 0 then k else count(n - 1, k)
  def steps(n: Int): Int = n match
    case 0 => 0
    case m => steps(m - 1)
  def ticks(step: Int): Int =
    @tailrec
    def tick(i: Int): Int = if i < 0 then i else tick(i - step)
    tick(1000000)
  def notTail(n: Int): Int = if n == 0 then 0 else 1 + notTail(n - 1)
  def len[A](l: List[A], n: Int): Int = l match
    case _ :: rest => len(rest, n + 1)
    case Nil => n

@main def tailrec(): Unit =
  println(T.fact(20, 1L))
  println(T.count(1000000, 7))
  println(T.steps(1000000))
  println(T.ticks(3))
  println(T.notTail(100))
  println(T.len(List.fill(100000)(0), 0))
  println(Chain(1, Chain(2, Chain(3, null))).last(5))
