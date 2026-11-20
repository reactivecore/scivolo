package refused

trait A extends Any {
  def b: B = new B(1)
}

class B(val n: Int) extends AnyVal with A

class C {
  def b: B = new B(2)
}

object AObj extends A

object Lib {
  def fbounded(x: T forSome { type T <: Comparable[T] }): Int = 0
  def struct(x: { def size: Int }): Int = x.size
  private[refused] def secret: Int = 7
}

object Outer {
  object Deep {
    def same(s: String): s.type = s
  }
}
