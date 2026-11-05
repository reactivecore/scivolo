trait Sized {
  def size: Int
  def twice(): Int = size * 2
}

trait Plus extends Sized {
  override def twice(): Int = super.twice() + 1
}

class Box(val size: Int) extends Plus

object Traits {
  def main(args: Array[String]): Unit = {
    val b = new Box(5)
    println(b.twice())
    val s: Sized = b
    println(s.twice())
    println(s.size)
  }
}
