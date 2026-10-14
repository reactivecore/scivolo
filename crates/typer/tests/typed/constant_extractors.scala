object Even { def unapply(s: String): Boolean = s.length % 2 == 0 }
object Five { def unapply(v: Int): Option[Int] = Some(v) }
object Same { def unapply[T](x: T): Option[T] = Some(x) }

object Test {
  def even = "even" match { case Even() => 1; case _ => 0 }
  def five = 5 match { case Five(n) => n }
  def same = 1 match { case Same(x) => x }
}
