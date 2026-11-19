object Test:
  trait Box[T]:
    def apply(f: T => T): Int
    def apply(s: String): Int

  def w: String = ""
  def w[T]: Box[T] = ???
  def w[T](x: String): Box[T] = ???

  def u: Box[Int] = ???
  def u(x: String): Int = 1

  def a = w[Int](_ + 1)
  def b = u(_ + 1)
