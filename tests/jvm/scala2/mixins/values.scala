import mixins.*

class Counted extends Counter with Valued

class Cached extends Lazy

object Values {
  def main(args: Array[String]): Unit = {
    val c = new Counted
    println(c.bump())
    println(c.bump())
    println(c.count)
    println(c.start)
    println(new Cached().cached)
  }
}
