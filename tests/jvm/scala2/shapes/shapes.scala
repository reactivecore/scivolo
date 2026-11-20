import shapes.*

object Shapes {
  def main(args: Array[String]): Unit = {
    val inc = Functor.lift[List, Int, Int](_ + 1)
    println(inc(List(1, 2)))
    println(Functor.either[String].map(Right(2): Either[String, Int])(_ * 3))
    val t = new Tools
    println(t.swap((1, 2)))
    println(t.greet())
    println(t.greet("me", punct = "?"))
    println(t.total(1, 2, 3))
    println(t.total(List(4, 5)*))
    var n = 0
    println(t.twice { n += 1; n })
    println(t.sizes(Map(1 -> 2)))
    println(t.longest(List("ab", "abc")))
    println(t.both("x"))
    println(t.checked)
    println(Tools.id(5))
    println(Tools.Nested.answer)
    println(new Tools.Nested.Counter(1).next)
    import Tools.RichInt
    println(3.squared)
    println(Strings.elems)
    println(Strings.first.length)
    val c: Container = Strings
    println(c.elems.size)
    val name: Name = "loud"
    println(shout(name))
    val r = Rect(2.0, 3.0)
    println(r.area)
    println(r.copy(h = 1.0).w)
    println(Dot.area)
    val s: Shape = r
    println(s.area)
    println(List(1).map(Pt(_)))
  }
}
