package java.lang

import scala.scalanative.runtime.GC

/** The collector's heap size and limit as total and maximum memory; free memory 0. */
abstract class Runtime {
  def totalMemory(): scala.Long = GC.getUsedHeapSize().toLong
  def maxMemory(): scala.Long = GC.getMaxHeapSize().toLong
  def freeMemory(): scala.Long = 0L
}
