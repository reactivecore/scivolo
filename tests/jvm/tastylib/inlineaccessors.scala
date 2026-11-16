package scala.probe

class Counter {
  private var n: Int = 0
  private def get: Int = n
  inline def bump(): Int = { n += 1; get }
}

object Counters {
  private val start = 1
  inline def first: Int = start
}

class Outer {
  object Inner {
    private var n: Int = 0
    inline def bump(): Int = { n += 1; n }
  }
}

class Base {
  protected def size: Int = 1
  protected var count: Int = 0
}

object Derived extends Base {
  inline def twice: Int = { count += 1; size * 2 }
}
