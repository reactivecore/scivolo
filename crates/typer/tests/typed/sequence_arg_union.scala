object Test {
  def m(xs: Any*): Int = xs.length
  def union(l: List[Int] | List[String]): Int = m(l*)
  def inter(l: List[Int] & List[String]): Int = m(l*)
  def arrays(a: Array[Int] | Array[String]): Int = m(a*)
  def nested(l: List[Int] | (Vector[Long] | Seq[String])): Int = m(l*)
  def withNull(l: List[Int] | Null): Int = m(l*)
  def ifs(p: Boolean, a: Array[Any], b: Array[Any]): Int = m((if p then a else b)*)
}
