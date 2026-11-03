trait A { def f[T](x: T): List[T] }
trait B { def f[U](x: U): Seq[U] }

trait Holder[+CC[+X, -Y, Z]] {
  def get: CC[String, Any, Int]
  def up: CC[Any, String, Int] = get
}

object Use {
  def ab(x: A & B): List[Int] = x.f(3)
  def ba(x: B & A): List[Int] = x.f(4)
}
