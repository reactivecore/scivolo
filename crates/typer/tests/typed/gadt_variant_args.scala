trait Store[K, +V]
class MapStore[K, V](val keys: List[K]) extends Store[K, V]

object GadtVariantArgs {
  def firstKey[K, V](s: Store[K, V]): Option[K] = s match {
    case m: MapStore[k, v] => m.keys.headOption
    case _ => None
  }
  def nullable[K, V](s: Store[K, V] | Null): Option[K] = s match {
    case m: MapStore[k, v] => m.keys.headOption
    case _ => None
  }
}
