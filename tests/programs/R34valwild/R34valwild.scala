//> using scala 3.9.0
case class R34Box[A](x: A):
  def flatMap[B](f: A => R34Box[B]): R34Box[B] = f(x)
  def map[B](f: A => B): R34Box[B] = R34Box(f(x))

object R34valwild:
  def main(args: Array[String]): Unit =
    val _ =
      for
        a <- R34Box(1)
        b <- R34Box(a + 1)
      yield a + b
    val r = R34Box(2).flatMap(a => R34Box(a * 3)) match
      case R34Box(n) => n
    println(r)
