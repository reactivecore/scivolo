trait Ops[K, +V, +CC[_, _], +C] {
  def keys: List[K]
  def keySet: List[K] =
    (this: @unchecked) match
      case s: SMap[K, V] => new LKS(s).get
      case _ => Nil
}
trait SMap[K, +V] extends Ops[K, V, SMap, SMap[K, V]]
class LKS[K, +V, +CC[_, _], +C](mp: Ops[K, V, CC, C]) {
  def get: List[K] = mp.keys
}
