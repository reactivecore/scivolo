//> using scala 3.9.0
object Test:
  def outer(k: Int): Int =
    def twice = k * 2
    def plain = 7
    twice + plain + twice

  def nested(s: String): String =
    def head = s.take(1)
    val f = () => head + head
    f()

  def branches(k: Int): Int =
    def twice = k * 2
    if k > 0 then twice else twice

@main def parameterless(): Unit =
  println(Test.branches(4))
  println(Test.outer(3))
  println(Test.nested("xy"))
