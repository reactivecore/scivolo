import valueclasses.*
import valueclasses.Ops.*

object ValueClasses {
  def main(args: Array[String]): Unit = {
    val a = new Meter(1.5)
    val b = Meter.of(2)
    println((a + b).show)
    println(a.scaled(3).v)
    println(a == new Meter(1.5))
    println(a.hashCode == new Meter(1.5).hashCode)
    println(Meter.zero.v)

    val n = Name("ann")
    println(n.greet)
    println(n)
    println(n == Name("ann"))
    println(n.copy(s = "bob").s)
    println(name(n))

    println(new Hidden(21).twice)

    val sq = new Square(3)
    println(sq.area)
    println(sq.describe)
    println(shapes(sq))
    val s: Shape = sq
    println(s.area)

    val box = new Box(20).map(_ + 1)
    println(box.get)
    println(boxed(box))
    println(new Box("x").map(_ * 2).a)

    println(new Chunk(Array("p", "q")).size)
    println(chunk(new Chunk(Array("r"))))

    println(new Holder(a).next.v)
    println("hey".shout)
    println(Shout("ho").shout)

    val ms = Array(a, b, new Meter(0.5))
    println(sum(ms).v)
    println(total(ms.toList))
    println(maybe(Some(b)).v)
    println(maybe(None).v)
    println(id(a).v)

    val any: Any = a
    any match {
      case m: Meter => println("meter " + m.v)
      case _ => println("other")
    }
  }
}
