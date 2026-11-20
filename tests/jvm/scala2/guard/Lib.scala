package guard

object O {
  def f(x: Int): Int = x + 1
}

class V(val i: Int) extends AnyVal {
  def g(x: Int): Int = i + x
}
