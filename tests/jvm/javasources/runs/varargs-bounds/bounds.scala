import scala.annotation.varargs

class Bounds {
  @varargs def ints[E <: Int](e: E*): Int = e.sum
  @varargs def doubles[E <: Double](e: E*): Double = e.sum
  @varargs def comparable[E <: Comparable[E]](e: E*): Int = e.length
  @varargs def both[E <: Comparable[E] & java.io.Serializable](e: E*): Int = e.length
  @varargs def anyVal[E <: AnyVal](e: E*): Int = e.length
  @varargs def any[E](e: E*): Int = e.length
}
