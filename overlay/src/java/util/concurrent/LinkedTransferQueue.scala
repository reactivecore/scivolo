// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); changed:
//   javalib/src/main/scala/java/util/concurrent/LinkedTransferQueue.scala
// LinkedTransferQueue.scala: written by Doug Lea with assistance from members of JCP JSR-166
// Expert Group and released to the public domain (CC0).
package java.util.concurrent

import java.util.AbstractQueue

/** `remove`, which reads `p.next` only while the inner loop continues. */
abstract class LinkedTransferQueue[E <: AnyRef] extends AbstractQueue[E] {
  import LinkedTransferQueue.Node

  private[concurrent] def head: Node = ???

  private def skipDeadNodes(pred: Node, c: Node, p: Node, q: Node): Node = ???

  override def remove(o: Any): Boolean = {
    if (o == null) return false
    var restartFromHead = true
    while (restartFromHead) {
      restartFromHead = false

      var p = head
      var pred: Node = null
      var innerBreak = false
      while (p != null && !innerBreak) {
        var q = p.next
        val item = p.item
        var skipRest = false
        if (item != null) {
          if (p.isData) {
            if (item.equals(o) && p.tryMatch(item, null)) {
              skipDeadNodes(pred, p, p, q)
              return true
            }
            pred = p
            p = q
            skipRest = true
          }
        } else if (!p.isData) innerBreak = true
        if (!skipRest && !innerBreak) {
          val c = p
          var cBreak = false
          while (!cBreak) {
            if (q == null || !q.isMatched()) {
              pred = skipDeadNodes(pred, c, p, q)
              p = q
              cBreak = true
            } else {
              val old_p = p
              p = q
              if (old_p == p) {
                innerBreak = true
                cBreak = true
                restartFromHead = true
              }
            }
            if (!cBreak) q = p.next
          }
        }
      }
    }
    false
  }
}

object LinkedTransferQueue {
  final class Node {
    def isData: Boolean = ???
    def item: Object = ???
    def next: Node = ???
    def isMatched(): Boolean = ???
    def tryMatch(cmp: Object, `val`: Object): Boolean = ???
  }
}
