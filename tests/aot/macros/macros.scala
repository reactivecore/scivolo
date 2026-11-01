import macrolib.Macros.*
import macrolib.Members.declarations
import scala.deriving.Mirror

case class Point(x: Int, y: String)

@main def macros(): Unit =
  println(twice(21))
  println(greet("Bob"))
  println(listOf('a', 'b'))
  val n = 4
  println(plusOneTwice(n))
  println(declarations[Point])
  println(incremented {
    case class Pair(a: Int, b: Int)
    val p = summon[Mirror.ProductOf[Pair]].fromProduct((1, 2))
    if p.hashCode == Pair(1, 2).hashCode && p == Pair(1, 2) then p.a + p.b else 0
  })
