//> using scala 3.9.0
import scala.annotation.varargs

class Counting(out: java.io.PrintStream) extends java.io.PrintStream(out):
  var calls = 0
  override def printf(format: String, args: Object*): java.io.PrintStream =
    calls += 1
    super.printf(format, args*)

class J:
  @varargs def join(sep: String, parts: String*): String = parts.mkString(sep)
  @varargs def total(xs: Int*): Int = xs.sum

@main def varargsforwarders(): Unit =
  val c = Counting(System.out)
  val p: java.io.PrintStream = c
  p.printf("%s-%s%n", "a", "b")
  c.printf("%s%n", "c")
  println(c.calls)
  val j = J()
  println(j.join(",", "x", "y"))
  println(j.total(1, 2, 3))
