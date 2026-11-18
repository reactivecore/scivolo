package generics

object ByLength extends java.util.Comparator[String] {
  def compare(a: String, b: String): Int = a.length - b.length
}

trait Marker
trait Context { def on[T](t: => T): T }

class Blocks {
  def context(): Context = new Context {
    def on[T](t: => T): T = {
      val blocker = new Marker with (() => T) { def apply(): T = t }
      blocker()
    }
  }
}

class Outer[TA] {
  class Mid {
    class In
  }
}
