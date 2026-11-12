package shapes

class Named extends Box[String]("named")

object Javajar {
  def main(args: Array[String]): Unit = {
    val b = new Box[String]("hi")
    println(b.get() + " " + Box.of(41).get())
    val p = new Box.Pair(1, 2)
    p.right = 5
    println(p.total() + p.left)
    println(Box.sum(1, 2, 3) + " " + Box.NAME + " " + Box.created)
    val circle: Box.Shape = () => 3.0
    println(Box.Shape.square(2.0).area() + circle.area())
    val inner: Box[String]#Inner = b.inner()
    println(new b.Inner().value() + " " + inner.value().length)
    println(new Box.Hidden().name())
    val c = new Counter
    c.next()
    println(c.next() + " " + c.count + " " + Counter.PREFIX.length)
    println(new Named().inner().value())
  }
}
