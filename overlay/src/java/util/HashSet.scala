package java.util

/** `containsAll` without javalib's lambda, which is compiled into `throw` (upstream bug 3). */
abstract class HashSet[E] extends AbstractSet[E] {
  override def contains(o: Any): scala.Boolean = ???

  override def containsAll(c: Collection[?]): scala.Boolean = {
    val it = c.iterator()
    while (it.hasNext()) if (!contains(it.next())) return false
    true
  }
}
