package java.util

/** `containsAll` as a loop over `contains`. */
abstract class HashSet[E] extends AbstractSet[E] {
  override def contains(o: Any): scala.Boolean = ???

  override def containsAll(c: Collection[?]): scala.Boolean = {
    val it = c.iterator()
    while (it.hasNext()) if (!contains(it.next())) return false
    true
  }
}
