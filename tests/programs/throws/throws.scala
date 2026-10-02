//> using scala 3.9.0
object Fail:
  def fail(s: String): Int = throw new RuntimeException(s)
  def check(i: Int): Int = if i < 0 then throw new IllegalArgumentException("negative") else i
  def first(b: Boolean): String =
    if b then return "early"
    throw new IllegalStateException("late")

@main def throws(): Unit =
  println(Fail.check(3))
  println(Fail.first(true))
