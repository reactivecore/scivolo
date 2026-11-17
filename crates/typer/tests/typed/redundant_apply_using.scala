class A:
  given Int = 3
  def w(using k: Int)(x: Int): Int = k + x
  def w2[T](using k: Int)(x: T): T = x

  def g = w.apply(1)
  def h = w2.apply("x")
