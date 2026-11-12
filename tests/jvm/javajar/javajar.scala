import shapes.Box

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
  }
}
