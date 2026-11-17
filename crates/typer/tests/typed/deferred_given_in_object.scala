//> using options -language:experimental.modularity -source future
import compiletime.deferred

trait Ord:
  type Self

trait A:
  type Elem
  given Elem is Ord = deferred

object Inst:
  given Int is Ord

object Test:
  import Inst.given
  object E extends A:
    type Elem = Int
  class C extends A:
    type Elem = Int
