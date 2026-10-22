//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Terms.*

object Terms:
  val one = 1
  final val two = 2
  def f(a: Int, b: Int): Int = a + b
  val a = shape(1)
  val b = shape("s")
  val c = shape(one)
  val d = shape(one + 2)
  val e = shape(f(1, b = 2))
  def sum(xs: Int*): Int = xs.sum
  val g = shape(sum(1, 2))
  val h = shape((1: Int))
  val i = shape { val x = 1; x }
  val j = shapeUnderlying({ 3 })
  val k = known(4)
  val l = known(two)
  val m = known(one)
  val n = knownText("abc")
  val p = shape(Option.empty[Int])
  val q = segments("a/b")
  val r = segments(n)
  val s = words("x y")
