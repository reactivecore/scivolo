package scala.probe

import language.experimental.captureChecking

trait Lazy[+A]

final class Proxy[A](of: Lazy[A]^) extends Serializable {
  @transient protected var coll: Lazy[A]^{this} = of
}
