//> using options -Yexplicit-nulls
package scala.probe

import scala.annotation.unchecked.uncheckedVariance

final class Cell[A](var tail: Cell[A] | Null)

final class Concat[+A](val from: Iterator[A @uncheckedVariance] | Null) {
  @annotation.stableNull
  private var current: Iterator[A] | Null = from
  @annotation.stableNull
  private var tail: Cell[A @uncheckedVariance] | Null = null
  @annotation.stableNull
  private var last: Cell[A @uncheckedVariance] | Null = null

  def merge(): Unit =
    if (current.isInstanceOf[Concat[?]]) {
      val c = current.asInstanceOf[Concat[A]]
      if (c.tail != null) {
        if (last == null) last = c.last
        tail = c.tail
      }
    }
}

trait Ops[+A, +CC[_], +C] {
  def iterator: Iterator[A]
  protected def fromSpecific(coll: IterableOnce[A @uncheckedVariance]): C
  protected def newBuilder: List[A @uncheckedVariance]
  def grouped(size: Int): Iterator[C] = iterator.grouped(size).map(fromSpecific)
  def from[E](it: IterableOnce[E]): CC[E]
  def zip[B](that: IterableOnce[B]): CC[(A @uncheckedVariance, B)] = from(that match {
    case that: Iterable[B @unchecked] => that.iterator.map(b => (iterator.next(), b))
    case _ => iterator.zip(that)
  })
}

object Patterns {
  def view[E](it: IterableOnce[E]): Iterable[E] = it match {
    case it: Iterable[E @unchecked] => it
    case it: Iterator[E @unchecked] => it.toList
    case _ => Nil
  }
  def node[K, V](that: Any): Any = that match {
    case hm: scala.collection.mutable.HashMap[K @unchecked, V @unchecked] =>
      val iter = hm.iterator
      iter.next()
    case lm: scala.collection.mutable.LinkedHashMap[K @unchecked, V @unchecked] => lm
    case _ => 0
  }
  def nullable[T](cs: java.util.concurrent.CompletionStage[T]): Any = {
    val completed = cs match {
      case cf0: java.util.concurrent.CompletableFuture[T @unchecked] =>
        val cf = cf0.toCompletableFuture
        if (cf.isDone) cf else null
      case _ => null
    }
    val again = cs match {
      case f: java.util.concurrent.Future[T @unchecked] => f
      case _ => None
    }
    (Some(completed), again)
  }
}
