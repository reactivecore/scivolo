package scala.probe

import annotlib.{Box, Lib}

class Uses(l: Lib, b: Box[Int]) {
  def tags = l.tags
  def first = l.tags.head
  def marks = l.marks
  def both = l.both(1)
  val pair = l.pair
  def marked = b.marked
  def tagged = b.tagged
  def static = b.static
  def full = b.full
}
