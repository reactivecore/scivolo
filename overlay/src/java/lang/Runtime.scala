package java.lang

import scala.scalanative.runtime.GC

/** Memory figures of the collector: heap size and limit; no count of live bytes, so no free memory. */
abstract class Runtime {
  def totalMemory(): scala.Long = GC.getUsedHeapSize().toLong
  def maxMemory(): scala.Long = GC.getMaxHeapSize().toLong
  def freeMemory(): scala.Long = 0L
}
