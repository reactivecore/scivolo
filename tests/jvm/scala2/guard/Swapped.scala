package guard

object O {
  def f(x: Long): Int = 0
}

class V(val i: Int) extends AnyVal {
  def g(x: Long): Int = i
}
