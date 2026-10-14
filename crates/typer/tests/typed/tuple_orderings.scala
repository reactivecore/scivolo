object Orderings {
  val pair = implicitly[Ordering[(Int, String)]]
  val triple = implicitly[Ordering[(Int, String, Double)]]
  def ranked(freq: Seq[(String, Int)]) = freq.sortBy((w, c) => (-c, w))
}
