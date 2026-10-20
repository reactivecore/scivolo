//> using scala 3.9.0
// Auto-tupling, varargs, later-clause defaults, extractor clauses, untupled closures, SAM types.
trait Show { def show(i: Int): String }

abstract class Joiner(parts: String*) { def join(s: String): String }

trait Box { type T; def put(t: T): String }

object Pair {
  def unapply(x: Int)(using s: Show): Option[(String, Int)] =
    if x > 0 then Some((s.show(x), x * 2)) else None
}

object Gen {
  def unapply(x: Int): Int *: String *: EmptyTuple = (x, x.toString)
}

class Later(a: Int)(b: Int = a + 1) { def sum: Int = a + b }

def lazily(xs: => Int*): Int = xs.sum

def pair(p: (Int, String)): String = p._2 * p._1

def later(a: => Int)(b: Int = a + 1): Int = a + b

def named(xs: Int*): Int = xs.length

@main def argclauses(): Unit =
  given Show = i => s"<$i>"
  var n = 0
  println(lazily({ n += 1; n }, { n += 1; n }))
  println(pair(3, "ab"))
  println(later { n += 10; n }())
  println(named(xs = Seq(1, 2, 3)*))
  println(Later(4)().sum)
  3 match
    case Pair(s, d) => println(s + d)
    case _ => println("none")
  7 match
    case Gen(1, s) => println("one " + s)
    case Gen(i, s) => println(s"$i $s")
  val f: ((Int, String)) => String = (i: Int, s: String) => s * i
  println(f((2, "xy")))
  val g: Int *: Int *: EmptyTuple => Int = (x, y) => x * y
  println(g((6, 7)))
  val j: Joiner = s => s + "!"
  println(j.join("hi"))
  val b: Box { type T = Int } = t => s"[$t]"
  println(b.put(5))
