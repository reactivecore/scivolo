package libforwarders

import scala.collection.immutable.{AbstractSeq, LinearSeq}

abstract class Ls[+A] extends AbstractSeq[A] with LinearSeq[A]

case object Nl extends Ls[Nothing]:
  override def apply(i: Int): Nothing = throw new IndexOutOfBoundsException
  override def length: Int = 0
  override def isEmpty: Boolean = true
  override def head: Nothing = throw new NoSuchElementException
  override def tail: Nl.type = throw new UnsupportedOperationException
