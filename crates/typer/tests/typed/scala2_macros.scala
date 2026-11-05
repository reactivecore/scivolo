object Main {
  def f(x: Int): Int = macro ???
  inline def f(x: Int): Int = x
  def g: Int = macro ???
  def g: Int = 2
}
