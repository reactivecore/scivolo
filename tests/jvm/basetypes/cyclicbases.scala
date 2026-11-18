trait Base[+T] { def get: T }
trait P1 extends Base[E]
trait P2 extends Base[Base[String]]
abstract class C extends P1 with P2
trait F extends Base[E]
abstract class E extends C with F

object CyclicBases {
  def viaC(c: C): Base[E & Base[String]] = c
  def viaE(e: E): Base[E & Base[String]] = e
}
