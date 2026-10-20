object Main {
  def run(): Unit = {
    O.f(33L)
    O.f(33d)
  }
}

object O {
  def f(n: Double) = "double"
  def f(n: Long) = "long"
}
