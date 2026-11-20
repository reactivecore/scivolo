package members

object O {
  lazy val lz: Int = { println("init"); 5 }
  final val c = 7
  implicit val ord: Ordering[String] = Ordering.String
  protected[members] def hidden: Int = 1
  def arr[T](xs: Array[T]): Int = xs.length
  def prim(xs: Array[Int]): Int = xs.length
  def unit(): Unit = println("unit")
  def curried(a: Int)(b: Int): Int = a + b
  def withOrdering(a: Int)(implicit o: Ordering[String]): Int = o.compare("a", "b") * a
  var v: String = "v"
}

class Gen[T <: AnyRef](val t: T) {
  def nul: Null = null
  def nothing: Nothing = throw new IllegalStateException("nothing")
}
