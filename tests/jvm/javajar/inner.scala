import shapes.Box

object Inner {
  def main(args: Array[String]): Unit = {
    val b = new Box[String]("hi")
    println(new b.Inner(b).value())
  }
}
