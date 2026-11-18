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

object Flows:
  def first[A, B](x: A, f: A => It[B]^): Boolean =
    var it: It[B]^{f} | Null = null
    it = f(x)
    it.hasNext
