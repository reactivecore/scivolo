trait T[K, +V] { def apply(k: K): V = ??? }
class M[K, +V] extends T[K, V]
object M {
  sealed trait Ob
  case object I extends Ob
  def empty[K, V]: M[K, V] = new M
  def empty[K, V](o: Ob): M[K, V] = new M
}
object Use {
  val m = M.empty(M.I)
  def n(o: M.Ob): M[String, Int] = M.empty(o)
}
