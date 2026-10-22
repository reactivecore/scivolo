object Closures {
  val xs = List(1, 2, 3)
  def a = xs.zipWithIndex.map(p => p._1)
  def b = xs.zipWithIndex.map(_._2)
  def c = xs.zipWithIndex.foreach(p => println(p))
}
