object Test {
  def m(xs: Any*): Int = xs.length
  def union(l: List[Int] | List[String]): Int = m(l*)
  def inter(l: List[Int] & List[String]): Int = m(l*)
  def nested(l: List[Int] | (Vector[Long] | Seq[String])): Int = m(l*)
  def withNull(l: List[Int] | Null): Int = m(l*)
  def arrayOrNull(a: Array[Int] | Null): Int = m(a*)
}
