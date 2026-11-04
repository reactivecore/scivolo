import language.experimental.captureChecking

final class Box[+T](val value: T) {
  def map[U](f: T => U): Box[U]^{f} = new Box(f(value))
}

object Boxes {
  def fill[A](n: Int)(elem: => A): Iterator[A]^{elem} = Iterator.fill(n)(elem)
}

object capturesets {
  def main(args: Array[String]): Unit = {
    var k = 0
    println(Boxes.fill(3) { k += 1; k }.toList)
    println(new Box(20).map(_ + 1).value)
  }
}
