object Test:
  trait Component[T]:
    inline def minimum: T = wrap(42)
    inline def wrap(value: Int): T
  opaque type Major = Int
  object MajorVersion extends Component[Major]:
    inline def wrap(value: Int): Major = value
    inline def reset: Major = minimum
  trait Increment:
    def increment: Major
  class MyIncrement extends Increment:
    inline def increment: Major = MajorVersion.reset

private object Foo:
  transparent inline def haha(): Unit = {}
export Foo.*

def main(): Unit = haha()
