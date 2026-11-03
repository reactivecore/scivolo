import scala.collection.immutable.{Map, SortedMapOps}

trait Ops[K, +V, +CC[X, +Y] <: Map[X, Y] & SortedMapOps[X, Y, CC, ?]] {
  def c: CC[K, V]
  def widened[V1 >: V]: CC[K, V1] = c
  def added[V1 >: V](kv: (K, V1)): CC[K, V1] = c.updated(kv._1, kv._2)
}
