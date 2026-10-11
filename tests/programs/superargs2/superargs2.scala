//> using scala 3.9.0
class Base(val a: Any):
  override def toString = "Base(" + a + ")"

class Sub() extends Base({ def bippy = 5; bippy })

class Scaled(k: Int, s: String) extends Base({ def f(x: Int) = x * k; f(s.length) })

class Fn(val f: () => Int)

object Crash extends Fn(() => {
  val n = 3
  n * 2
})

class Two(k: Int) extends Base(List(1, 2).map(_ + k).sum)

@main def superargs2(): Unit =
  println(new Sub())
  println(new Scaled(3, "four"))
  println(Crash.f())
  println(new Two(10))
