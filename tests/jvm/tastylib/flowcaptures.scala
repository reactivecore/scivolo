//> using options -Yexplicit-nulls
package scala.probe

import language.experimental.captureChecking
import scala.annotation.nowarn

abstract class It[+A]:
  def hasNext: Boolean
  def next(): A

final class Conc[+A](val from: (It[A @annotation.unchecked.uncheckedVariance]^) | Null) extends It[A]:
  @annotation.stableNull
  private var current: It[A]^{from} | Null = from
  def hasNext =
    if current == null then false
    else if current.hasNext then true
    else
      current = null
      false
  def next(): A =
    if current != null then current.next() else ???

trait Ops[+A]:
  self: Ops[A]^ =>
  @deprecated("no", "1") def toIt: It[A]^{this}
  def size(i: It[Any]^): Int
  def sized: Int = size(toIt: @nowarn("cat=deprecation"))

trait Coll:
  def iterator: It[Int]^{this}

class Holder(val c: Coll^):
  val it: It[Int]^{c} | Null = null
  def own = if it != null then it.next() else 0
  def other(o: Holder^) = if o.it != null then o.it.next() else 0
  class In(val d: Coll^{c}):
    val j: It[Int]^{d} | Null = null
    def inner = if j != null then j.next() else 0
    def sibling(o: In^) = if o.j != null then o.j.next() else 0

class SubHolder(x: Coll^) extends Holder(x):
  def inherited = if it != null then it.next() else 0

object Paths:
  def from(o: Holder^) = if o.it != null then o.it.next() else 0
  def local(c: Coll^) =
    val d: Coll^{c} = c
    val v: It[Int]^{d} | Null = d.iterator
    if v != null then v.next() else 0
  def localClass(c: Coll^) =
    class K(val d: Coll^{c}):
      val v: It[Int]^{d} | Null = null
      def g = if v != null then v.next() else 0
    new K(c).g
  def closure(c: Coll^) = (x: Coll^{c}) =>
    val v: It[Int]^{x} | Null = x.iterator
    if v != null then v.next() else 0

object Flows:
  def first[A, B](x: A, f: A => It[B]^): Boolean =
    var it: It[B]^{f} | Null = null
    it = f(x)
    it.hasNext
  def forced[A](it: (It[A]^) | Null): It[A]^{it} = it.nn
