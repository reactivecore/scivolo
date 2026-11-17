class A:
  def f(x: Int)(y: Int): Int = x + y
  def h[T](x: T)(y: Int): T = x
  def r[K](k: Int => K)(c: (K, K) => K): K = c(k(1), k(2))
  val fn = (x: Int) => x

  def g = f(22).apply(33)
  def k = h("a").apply(1)
  def l = r(x => x.toString).apply((a, b) => a + b)
  def m = f.apply(1)
  def n = fn.apply(2)
