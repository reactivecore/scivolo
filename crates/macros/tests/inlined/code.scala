//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Terms.*

object Code:
  val one = 1
  def f(a: Int, b: Int): Int = a + b
  val a = code(1 + one)
  val b = code(f(1, b = 2))
  val c = code(List(1, 2, 3).map(x => x * 2))
  val d = code { val x = 1; val y = "a\tb"; if x > 0 then y else "c" }
  val e = code(Option(1) match { case Some(n) if n > 0 => n; case _ => 0 })
  val g = code((x: Int) => (x: Int) => x)
  val h = code('c'.toInt + 2L + 1.5)
  val i = code { var s = 0; while s < 3 do s += 1; s }
  val j = code(try new StringBuilder("\u0001x") catch { case e: Exception => throw e } finally println())
  val k = code(Code.this.one :: Nil)
  val l = code { def g[T](t: T): T = t; g(one); List(1).foldLeft(0)((s, x) => { val y = s; y + x }) }
  val m = code(List(1, 2).sum(using Numeric.IntIsIntegral) - 'a'.toInt)
