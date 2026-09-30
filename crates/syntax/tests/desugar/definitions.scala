package p
def f[T: Ordering](x: Int = 1)(y: String = "a"): Int = x
extension [A](a: A) def +:(b: List[A]): List[A] = b
trait C:
  type T: Ordering
class D
