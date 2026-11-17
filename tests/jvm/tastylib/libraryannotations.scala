package scala.probe

import annotlib.{Box, Lib, tagged}

class Uses(l: Lib, b: Box[Int]) {
  def own: Option[String @tagged(3)] = None
  def tags = l.tags
  def first = l.tags.head
  def fives = l.fives
  def five = l.fives.head
  def threes = l.threes
  def marks = l.marks
  def both = l.both(1)
  val pair = l.pair
  def marked = b.marked
  def tagged = b.tagged
  def static = b.static
  def full = b.full
  def ownAfter: List[String @tagged(5)] = Nil
}
