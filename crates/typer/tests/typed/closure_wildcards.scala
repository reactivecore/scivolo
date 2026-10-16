object ClosureWildcards:
  def sizes(xs: List[List[?]]): List[Int] = xs.map(_.size)
  val length: (? >: String) => Int = s => s.length
  val same: String => (? <: CharSequence) = s => s
