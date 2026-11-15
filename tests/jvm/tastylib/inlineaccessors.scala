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
