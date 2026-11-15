package shapes

class Named extends Box[String]("named")

class Plate extends Box.Shape {
  def area: Double = 2.0
}

package extra {
  class Sub extends shapes.Box[String]("sub") {
    def prot: Int = shapes.Box.sprot()
  }

  object Shelves {
    def shelf: Int = new shapes.Box.Crate().shelf()
  }
}

object Javajar {
  @Box.Tag("t") def tagged = 0
  @Box.Tag(value = "u", level = 2, kind = classOf[String], tags = Array("a", "b"),
    where = java.lang.annotation.ElementType.METHOD) def tagged2 = 0

  def tag(name: String): String = {
    val t = Class.forName("shapes.Javajar").getMethod(name).getAnnotation(classOf[Box.Tag])
    s"${t.value()} ${t.level()} ${t.kind().getSimpleName} ${t.tags().mkString(",")} ${t.where()}"
  }

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
    println(new extra.Sub().prot)
    println(b.get + " " + new Plate().area() + " " + new Plate().area)
    println(new Box.Crate().shelf() + extra.Shelves.shelf)
    println(tag("tagged"))
    println(tag("tagged2"))
  }
}
