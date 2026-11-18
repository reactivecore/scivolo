import scala.collection.immutable.*
import scala.collection.{SortedMapFactory, SortedMapFactoryDefaults, generic}

abstract class InheritedResults[K, +V]
    extends AbstractMap[K, V]
    with SortedMap[K, V]
    with StrictOptimizedSortedMapOps[K, V, InheritedResults, InheritedResults[K, V]]
    with SortedMapFactoryDefaults[K, V, InheritedResults, Iterable, Map]
    with generic.DefaultSerializable:
  override def sortedMapFactory: SortedMapFactory[InheritedResults] = ???
  override def slice(from: Int, until: Int) = ???
  override def tail = ???
  override def init = ???
  override def take(n: Int) = ???
