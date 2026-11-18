package generics

object ByLength extends java.util.Comparator[String] {
  def compare(a: String, b: String): Int = a.length - b.length
}

class Outer[TA] {
  class Mid {
    class In
  }
}
